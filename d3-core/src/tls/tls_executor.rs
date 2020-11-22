use self::collective::*;
use self::scheduler::setup_teardown::Server;
use self::task::*;
use super::*;
// This is the TLS data for the executor. It is used by the channel and the executor;
// Otherwise, this would be much higher in the stack.

// A task for the scheduler, which will reschedule the machine
pub struct SchedTask {
    pub start: Instant,
    pub machine_key: usize,
}
impl SchedTask {
    pub fn new(machine_key: usize) -> Self {
        Self {
            start: Instant::now(),
            machine_key,
        }
    }
}

/// The ExecutorStats expose metrics for each executor.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutorStats {
    pub id: usize,
    pub tasks_executed: u128,
    pub instructs_sent: u128,
    pub blocked_senders: u128,
    pub max_blocked_senders: usize,
    pub exhausted_slice: u128,
    pub recv_time: std::time::Duration,
    pub time_on_queue: std::time::Duration,
    pub disturbed_nap: u128,
    pub sleep_count: u128,
    pub sleep_time: std::time::Duration,
}

// The state of the executor
#[derive(Copy, Clone, Eq, PartialEq, SmartDefault, Debug)]
pub enum ExecutorState {
    #[default]
    Init,
    Drain,
    Parked,
    Running,
}

// Encapsualted send errors
#[doc(hidden)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError {
    // The message could not be sent because the channel is full and the operation timed out.
    Full,
    // The message could not be sent because the channel is disconnected.
    Disconnected,
}

// Analogous the the ShareableMachine, the MachineSenderAdapter encapsulates
// and adapter containing a wrapped MachineDependentSenderAdapter, which encapsulates Sender<T> and T.
#[doc(hidden)]
pub struct MachineSenderAdapter {
    id: Uuid,
    key: usize,
    state: SharedMachineState,
    // the normalized_adapter is an ugly trait object, which needs fixing
    normalized_adapter: Box<dyn MachineDependentSenderAdapter>,
}
impl MachineSenderAdapter {
    pub fn new(machine: &ShareableMachine, adapter: Box<dyn MachineDependentSenderAdapter>) -> Self {
        Self {
            id: machine.get_id(),
            key: machine.get_key(),
            state: machine.state.clone(),
            normalized_adapter: adapter,
        }
    }
    // Get the id of the sending machine
    pub const fn get_id(&self) -> Uuid { self.id }
    // Get the key of the sending machine
    pub const fn get_key(&self) -> usize { self.key }
    // Try to send the message
    pub fn try_send(&mut self) -> Result<usize, TrySendError> { self.normalized_adapter.try_send() }
}

#[doc(hidden)]
pub trait MachineDependentSenderAdapter {
    fn try_send(&mut self) -> Result<usize, TrySendError>;
}

// This is information that the executor thread shares with the worker, allowing
// the big executor insight into what the executor is up to.
#[derive(Debug)]
pub struct SharedExecutorInfo {
    state: AtomicCell<ExecutorState>,
}
impl SharedExecutorInfo {
    pub fn set_state(&self, new: ExecutorState) { self.state.store(new); }
    pub fn get_state(&self) -> ExecutorState { self.state.load() }
    pub fn compare_set_state(&self, old: ExecutorState, new: ExecutorState) { self.state.compare_and_swap(old, new); }
}
impl Default for SharedExecutorInfo {
    fn default() -> Self {
        Self {
            state: AtomicCell::new(ExecutorState::Init),
        }
    }
}

// ExecutorData is TLS for the executor. Among other things, it provides bridging
// for the channel to allow a sender to park, while allowing the executor to continue
// processing work.
#[doc(hidden)]
#[derive(SmartDefault)]
pub struct ExecutorData {
    pub id: usize,
    pub task_id: usize,
    pub machine: ShareableMachine,
    pub blocked_senders: Vec<MachineSenderAdapter>,
    pub last_blocked_send_len: usize,
    pub notifier: ExecutorDataField,
    pub shared_info: Arc<SharedExecutorInfo>,
    pub run_queue: ExecutorDataField,
}
impl ExecutorData {
    pub fn block_or_continue() {
        tls_executor_data.with(|t| {
            let mut tls = t.borrow_mut();
            // main thread can always continue and block
            if tls.id == 0 {
                return;
            }
            // executor thread can continue if machine is in Running state
            let machine = &tls.machine;
            if !machine.is_running() {
                if !machine.is_send_blocked() {
                    log::error!(
                        "block_or_continue: expecting Running or SendBlock, found {:#?}",
                        machine.get_state()
                    );
                }
                // this executor is idle until the send that is in progress completes
                // stacking it would transform a bounded queue into an unbounded queue -- so don't stack
                tls.recursive_block();
            }
        });
    }
    pub fn recursive_block(&mut self) {
        // we're called from a tls context, ExecutorData is for the current thread.
        // we've already queue'd the sender, and now its trying to send more. This
        // could go on forever, essentially blocking an executor. So, we're going
        // to pause and drain this executor and then allow the send, that got us
        // here, to continue, having sent the one that blocked it

        log::debug!(
            "recursive_block begin exec {}, machine {}, state {:#?}",
            self.id,
            self.machine.get_key(),
            self.machine.get_state()
        );

        // if running, change to drain
        self.shared_info.compare_set_state(ExecutorState::Running, ExecutorState::Drain);

        self.drain();
        // when drain returns, set back to running
        self.shared_info.compare_set_state(ExecutorState::Drain, ExecutorState::Running);

        log::debug!(
            "recursive_block end exec {}, machine {}, state {:#?}",
            self.id,
            self.machine.get_key(),
            self.machine.get_state()
        );
    }

    pub fn sender_blocked(&mut self, channel_id: usize, adapter: MachineSenderAdapter) {
        // we're called from a tls context, ExecutorData is for the current thread.
        // upon return, the executor will return back into the channel send, which will
        // complete the send, which is blocked. Consequently, we need to be careful
        // about maintaining send order on a recursive entry.
        if adapter.state.load() == MachineState::SendBlock {
            // if we are already SendBlock, then there is send looping within the
            // machine, and we need to use caution
            log::info!("Executor {} detected recursive send block, this should not happen", self.id);
            unreachable!("block_or_continue() should be called to prevent entering sender_blocked with a blocked machine")
        }

        // otherwise we can stack the incomplete send. Depth is a concern.
        // the sends could be offloaded, however it has the potential to
        // cause a problem with the afformentioned looping sender.
        let machine = &self.machine;
        log::trace!(
            "executor {} machine {} state {:#?} parking sender {} task_id {}",
            self.id,
            machine.get_key(),
            machine.get_state(),
            channel_id,
            self.task_id,
        );

        if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::SendBlock) {
            log::error!("sender_block: expected state Running, found machine state {:#?}", state);
            log::error!(
                "sender_block: expected state Running, found adapter state {:#?}",
                adapter.state.load()
            );
            adapter.state.store(MachineState::SendBlock);
        }
        self.blocked_senders.push(adapter);
    }

    fn drain(&mut self) {
        use MachineState::*;
        // all we can do at this point is attempt to drain out sender queue
        let machine_key = self.machine.get_key();
        let machine_state = self.machine.state.clone();

        let mut start_len = 0;
        log::trace!("exec {} drain blocked {}", self.id, start_len);
        let backoff = LinearBackoff::new();
        while !self.blocked_senders.is_empty() {
            if start_len != self.blocked_senders.len() {
                start_len = self.blocked_senders.len();
                log::trace!("exec {} drain blocked {}", self.id, start_len);
            }
            let mut still_blocked: Vec<MachineSenderAdapter> = Vec::with_capacity(self.blocked_senders.len());
            let mut handled_recursive_sender = false;
            for mut sender in self.blocked_senders.drain(..) {
                match sender.try_send() {
                    // handle the blocked sender that got us here
                    Ok(_receiver_key) if sender.key == machine_key => {
                        backoff.reset();
                        // log::debug!("drain recursive machine ok state {:#?}", machine_state.get());
                        if let Err(state) = machine_state.compare_exchange(SendBlock, Running) {
                            log::error!("drain: expected state Running, found state {:#?}", state);
                            machine_state.store(Running);
                        }
                        handled_recursive_sender = true;
                    },
                    Err(TrySendError::Disconnected) if sender.key == machine_key => {
                        backoff.reset();
                        // log::debug!("drain recursive machine disconnected state {:#?}", machine_state.get());
                        if let Err(state) = machine_state.compare_exchange(SendBlock, Running) {
                            log::debug!("drain: expected state Running, found state {:#?}", state);
                            machine_state.store(Running);
                        }
                        handled_recursive_sender = true;
                    },
                    // handle all others
                    Ok(receiver_key) => {
                        backoff.reset();
                        // let the scheduler know that this machine can now be scheduled
                        // log::debug!("drain recursive other machine ok state {:#?}", machine_state.get());
                        match &self.notifier {
                            ExecutorDataField::Notifier(obj) => {
                                obj.notify_can_schedule_sender(sender.key);
                                obj.notify_can_schedule_receiver(receiver_key);
                            },
                            _ => log::error!("can't notify scheduler!!!"),
                        };
                    },
                    Err(TrySendError::Disconnected) => {
                        backoff.reset();
                        // log::debug!("drain recursive other machine disconnected state {:#?}", machine_state.get());
                        // let the scheduler know that this machine can now be scheduled
                        match &self.notifier {
                            ExecutorDataField::Notifier(obj) => obj.notify_can_schedule_sender(sender.key),
                            _ => log::error!("can't notify scheduler!!!"),
                        };
                    },
                    Err(TrySendError::Full) => {
                        still_blocked.push(sender);
                    },
                }
            }
            self.blocked_senders = still_blocked;
            if handled_recursive_sender {
                break;
            }
            // if we haven't worked out way free, then we need to notify that we're kinda stuck
            // even though we've done that, we may yet come free. As long as we're not told to
            // terminate, we'll keep running.
            if backoff.is_completed() && self.shared_info.get_state() != ExecutorState::Parked {
                // we need to notify the monitor that we're essentially dead.
                self.shared_info.set_state(ExecutorState::Parked);
                match &self.notifier {
                    ExecutorDataField::Notifier(obj) => obj.notify_parked(self.id),
                    _ => log::error!("Executor {} doesn't have a notifier", self.id),
                };
            }
            backoff.snooze();
        }
        log::debug!("drained recursive sender, allowing send to continue");
    }

    pub fn schedule(machine: ShareableMachine) {
        tls_executor_data.with(|t| {
            let tls = t.borrow();
            if tls.id != 0 {
                if log_enabled!(log::Level::Trace) {
                    let tls_machine = &tls.machine;
                    log::trace!(
                        "exec {} machine {} is scheduling machine {}",
                        tls.id,
                        tls_machine.get_key(),
                        machine.get_key()
                    );
                }
            } else {
                log::trace!("exec {} machine main-thread is scheduling machine {}", tls.id, machine.get_key());
            }
            if let ExecutorDataField::RunQueue(run_q) = &tls.run_queue {
                schedule_machine(machine, run_q);
            } else {
                // gotta do this the hard way
                if let Ok(run_q) = Server::get_run_queue() {
                    schedule_machine(machine, &run_q);
                } else {
                    log::error!("unable to obtain run_queue");
                }
            }
        });
    }
}

// Encoding the structs as a variant allows it to be stored in the TLS as a field.
#[doc(hidden)]
#[derive(SmartDefault)]
pub enum ExecutorDataField {
    #[default]
    Uninitialized,
    Notifier(ExecutorNotifierObj),
    Machine(ShareableMachine),
    RunQueue(ExecutorInjector),
}

// The trait that allows the executor to perform notifications
pub trait ExecutorNotifier: Send + Sync + 'static {
    // Send a notificiation that the executor is parked
    fn notify_parked(&self, executor_id: usize);
    // Send a notification that a parked sender is no long parked, and can be scheduled
    fn notify_can_schedule_sender(&self, machine_key: usize);
    // Send a notification that a parked sender's receiver may need to be scheduled
    fn notify_can_schedule_receiver(&self, machine_key: usize);
}
pub type ExecutorNotifierObj = std::sync::Arc<dyn ExecutorNotifier>;

thread_local! {
    #[doc(hidden)]
    #[allow(non_upper_case_globals)]
    pub static tls_executor_data: RefCell<ExecutorData> = RefCell::new(ExecutorData::default());
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)] use super::*;
}
