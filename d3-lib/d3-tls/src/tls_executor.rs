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

///
/// ExecutorData is TLS for the executor. Among other things, it provides bridging
/// for the channel to allow a sender to park, while allowing the executor to continue
/// processing work
#[derive(Default)]
pub struct ExecutorData {
    pub id: usize,
    pub state: ExecutorState,
    pub machine_id: u128,
    pub machine_state: MachineState,
    pub blocked_senders: Vec<SharedCollectiveSenderAdapter>,
    pub last_blocked_send_len: usize,
    pub notifier: ExecutorDataField,
}
impl ExecutorData {
    pub fn sender_blocked(&mut self, adapter: SharedCollectiveSenderAdapter) {
        // we're called from a tls context, ExecutorData is for the current thread.
        // upon return, the executor will return back into the channel send, which will
        // complete the send, which is blocked. Consequently, we need to be careful
        // about maintaining send order on a recursive entry.
        if adapter.state.get() == CollectiveState::SendBlock {
            // if we are already SendBlock, then there is send looping within the
            // machine, and we need to use caution
            log::info!("Executor {} detected recursive send block", self.id);
            // this means we have to suspend the executor
            // and launch a replacement.
            match &self.notifier {
                ExecutorDataField::Notifier(obj) => obj.notify_parked(self.id),
                _ => log::error!("Executor {} can't notify monitor!!!", self.id),
            };
            // at this point, all the executor should do is drain outstanding
            // sends (by compleing them) and then terminate.
            self.state = ExecutorState::Parked;
            self.drain(adapter);
        } else {
            // otherwise we can stack the incomplete send. Depth is a concern.
            // the sends could be offloaded, however it has the potential to
            // cause a problem with the afformentioned looping sender.
            adapter.state.set(CollectiveState::SendBlock);
            self.blocked_senders.push(adapter);
        }
    }

    /// Drain the executor, by completing all of the outstanding sends. Yes, this
    /// could use a good refactorng. Fortunately, it seldom runs.
    pub fn drain(&mut self, secondary: SharedCollectiveSenderAdapter) {
        // we're just  going to sit here attempting to drain the blocked
        // senders queue.
        log::warn!("Executor {} is draining {} sends", self.id, self.blocked_senders.len() );
        let secondary_id = secondary.id;
        let mut secondary = Some(secondary);
        let backoff = crossbeam::utils::Backoff::new();
        while !self.blocked_senders.is_empty() {
                let mut still_blocked: Vec::<SharedCollectiveSenderAdapter> = Vec::with_capacity(self.blocked_senders.len());
                for mut sender in self.blocked_senders.drain(..) {
                    match sender.try_send() {
                        Ok(()) => {
                            if secondary.is_some() {
                                if sender.id == secondary_id {
                                    let adapter = secondary.take().unwrap();
                                    still_blocked.push(adapter);
                                } else {
                                    match &self.notifier {
                                        ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                                        _ => log::error!("can't notify scheduler!!!"),
                                    };
                                }
                            } else {
                                match &self.notifier {
                                    ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                                    _ => log::error!("can't notify scheduler!!!"),
                                };
                            }
                            backoff.reset();
                        },
                        Err(TrySendError::Disconnected) => {
                            if secondary.is_some() {
                                if sender.id == secondary_id {
                                    let adapter = secondary.take().unwrap();
                                    still_blocked.push(adapter);
                                } else {
                                    match &self.notifier {
                                        ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                                        _ => log::error!("can't notify scheduler!!!"),
                                    };
                                }
                            } else {
                                match &self.notifier {
                                    ExecutorDataField::Notifier(obj) => obj.notify_can_schedule(sender.id),
                                    _ => log::error!("can't notify scheduler!!!"),
                                };
                            }
                            backoff.reset();
                        },
                        Err(TrySendError::Full) => {still_blocked.push(sender);},
                    };
                }
                backoff.snooze();
        }
        self.last_blocked_send_len = 0;
        log::info!("executor {} drained.", self.id);
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