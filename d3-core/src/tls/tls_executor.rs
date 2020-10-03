use super::*;

///
/// This is the TLS data for the executor. It is used by the channel and the executor;
/// Otherwise, this would be much higher in the stack.
/// 

///
/// The state of the machine.
/// All machines start New.
/// A disconnected machine hasn't been told that its disconnected, onceit is its dead.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SmartDefault)]
#[allow(dead_code)]
pub enum CollectiveState {
    #[default]
    New,
    Waiting,
    Ready,
    Running,
    SendBlock,
    RecvBlock,
    Disconnected,
    Dead,
}

/// A thread-safe wrapped state, which can be cloned.
pub type MachineState = SharedProtectedObject<CollectiveState>;

///
/// The state of the executor
#[derive(Copy, Clone, Eq, PartialEq, SmartDefault, Debug)]
pub enum ExecutorState {
    #[default]
    Init,
    Drain,
    Parked,
    Running,
}

///
/// Encapsualted send errors
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError {
    /// The message could not be sent because the channel is full and the operation timed out.
    Full,
    /// The message could not be sent because the channel is disconnected.
    Disconnected,
}

/// Analogous the the SharedCollectiveAdapter, the SharedCollectiveSenderAdapter encapsulates
/// and adapter containing a wrapped CollectiveSenderAdapter, which encapsulates Sender<T> and T.
pub struct SharedCollectiveSenderAdapter {
    pub id: u128,
    pub state: MachineState,
    // the normalized_adapter is an ugly trait object, which needs fixing
    pub normalized_adapter: CommonCollectiveSenderAdapter,
}
impl SharedCollectiveSenderAdapter {
    /// Get the id of the sending machine
    pub const fn get_id(&self) -> u128 { self.id }
    /// Try to send the message
    pub fn try_send(&mut self) -> Result<(), TrySendError> {
        self.normalized_adapter.try_send()
    }
}

pub trait CollectiveSenderAdapter {
    /// Get the id of the sending machine
    fn get_id(&self) -> u128;
    /// Try to send the message
    fn try_send(&mut self) -> Result<(), TrySendError>;
}
pub type CommonCollectiveSenderAdapter = Box<dyn CollectiveSenderAdapter>;


/// This is information that the executor thread shares with the worker, allowing
/// the big executor insight into what the executor is up to.
#[derive(Debug)]
pub struct SharedExecutorInfo {
    state: ExecutorState,
    start_idle: Instant,
}
impl SharedExecutorInfo {
    pub fn set_idle(&mut self) -> Instant { self.start_idle = Instant::now(); self.start_idle }
    pub fn set_state(&mut self, new: ExecutorState) { self.state = new }
    pub fn get_state(&self) -> ExecutorState { self.state }
    pub fn compare_set_state(&mut self, old: ExecutorState, new: ExecutorState) {
        if self.state == old { self.state = new }
    }
    pub fn get_state_and_elapsed(&self) -> (ExecutorState, Duration) {
        (self.state, self.start_idle.elapsed())
    }
}
impl Default for SharedExecutorInfo {
    fn default() -> Self { Self { state: ExecutorState::Init, start_idle: Instant::now( )}}
}

///
/// ExecutorData is TLS for the executor. Among other things, it provides bridging
/// for the channel to allow a sender to park, while allowing the executor to continue
/// processing work
#[derive(Default)]
pub struct ExecutorData {
    pub id: usize,
    pub machine_id: u128,
    pub machine_state: MachineState,
    pub blocked_senders: Vec<SharedCollectiveSenderAdapter>,
    pub last_blocked_send_len: usize,
    pub notifier: ExecutorDataField,
    pub shared_info: Arc<Mutex<SharedExecutorInfo>>,
}
impl ExecutorData {
    pub fn recursive_block(&mut self) {
        // we're called from a tls context, ExecutorData is for the current thread.
        // we've already queue'd the sender, and now its trying to send more. This
        // could go on forever, essentially blocking an executor. So, we're going
        // to pause and drain this executor and then allow the send, that got us
        // here, to continue, having sent the one that blocked it

        // if running, change to drain
        self.shared_info.lock().as_mut().unwrap().compare_set_state(ExecutorState::Running, ExecutorState::Drain);
        self.drain();
        let mut mutable = self.shared_info.lock().unwrap();
        // when drain returns, set back to running and reset idle
        mutable.compare_set_state(ExecutorState::Drain, ExecutorState::Running);
        mutable.set_idle();
    }
    pub fn sender_blocked(&mut self, channel_id: usize, adapter: SharedCollectiveSenderAdapter) {
        // we're called from a tls context, ExecutorData is for the current thread.
        // upon return, the executor will return back into the channel send, which will
        // complete the send, which is blocked. Consequently, we need to be careful
        // about maintaining send order on a recursive entry.
        if adapter.state.get() == CollectiveState::SendBlock {
            // if we are already SendBlock, then there is send looping within the
            // machine, and we need to use caution
            log::info!("Executor {} detected recursive send block, this should not happen", self.id);
            unreachable!("block_or_continue() should be called to prevent entering sender_blocked with a blocked machine")
        } else {
            // otherwise we can stack the incomplete send. Depth is a concern.
            // the sends could be offloaded, however it has the potential to
            // cause a problem with the afformentioned looping sender.
            log::trace!("executor {} parking sender {}", self.id, channel_id);
            adapter.state.set(CollectiveState::SendBlock);
            self.blocked_senders.push(adapter);
        }
    }
    fn drain(&mut self) {
        // all we can do at this point is attempt to drain out sender queue
        let backoff = crossbeam::utils::Backoff::new();
        while !self.blocked_senders.is_empty() {
            self.shared_info.lock().as_mut().unwrap().set_idle();
            let mut still_blocked: Vec::<SharedCollectiveSenderAdapter> = Vec::with_capacity(self.blocked_senders.len());
            let mut handled_recursive_sender = false;
            for mut sender in self.blocked_senders.drain(..) {
                match sender.try_send() {
                    // handle the blocked sender that got us here
                    Ok(()) if sender.id == self.machine_id => {
                        backoff.reset();
                        self.machine_state.set(CollectiveState::Running);
                        handled_recursive_sender = true;
                    },
                    Err(TrySendError::Disconnected) if sender.id == self.machine_id => {
                        backoff.reset();
                        self.machine_state.set(CollectiveState::Running);
                        handled_recursive_sender = true;
                    },
                    // handle all others
                    Ok(()) => {
                        backoff.reset(); 
                        // let the scheduler know that this machine can now be scheduled
                        match &self.notifier {
                            ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                            _ => log::error!("can't notify scheduler!!!"),
                        };
                    }
                    Err(TrySendError::Disconnected) => {
                        backoff.reset(); 
                        // let the scheduler know that this machine can now be scheduled
                        match &self.notifier {
                            ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                            _ => log::error!("can't notify scheduler!!!"),
                        }; 
                    },
                    Err(TrySendError::Full) => {still_blocked.push(sender);},
                }
            }
            self.blocked_senders = still_blocked;
            if handled_recursive_sender { break }
            // if we haven't worked out way free, then we need to notify that we're kinda stuck
            // even though we've done that, we may yet come free. As long as we're not told to
            // terminate, we'll keep running.
            if backoff.is_completed() && self.shared_info.lock().unwrap().get_state() != ExecutorState::Parked {
                // we need to notify the monitor that we're essentially dead.
                self.shared_info.lock().as_mut().unwrap().set_state(ExecutorState::Parked);
                match &self.notifier {
                    ExecutorDataField::Notifier(obj) => obj.notify_parked(self.id),
                    _ => log::error!("Executor {} doesn't have a notifier", self.id),
                };        
            }
            backoff.snooze();
        }
        log::debug!("drained recursive sender, allowing send to continue");
    }
}

/// Encoding the notifier as a variant allows it to be stored in the TLS as a field.
#[derive(SmartDefault)]
pub enum ExecutorDataField {
    #[default]
    Uninitialized,
    Notifier(ExecutorNotifierObj),
}

/// The trait that allows the executor to perform notifications
pub trait ExecutorNotifier: Send + Sync + 'static {
    /// Send a notificiation that the executor is parked
    fn notify_parked(&self, executor_id: usize);
    /// Send a notification that a parked sender is no long parked, and can be scheduled
    fn notify_can_schedule(&self, machine_id: u128);
}
pub type ExecutorNotifierObj = std::sync::Arc<dyn ExecutorNotifier>;


thread_local! {
    #[allow(non_upper_case_globals)]
    pub static tls_executor_data: RefCell<ExecutorData> = RefCell::new(ExecutorData::default());
}



#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
}