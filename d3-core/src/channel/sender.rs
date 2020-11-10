use self::connection::*;
use super::*;
use crossbeam::channel::{SendError, SendTimeoutError, TrySendError};

/// Wrap the crossbeam sender to allow the executor to handle a block.
/// This requires that a send which would block, parks the send. It
/// also requires that prior to sending a check is made to determine
/// if the sender is already blocked. What makes this work is that the
/// repeat send is bound to the executor. Consequently, TLS data can
/// be inspected to determine if we need to not complete the send.
///
/// Otherwise, the Sender is a wrapper aruond the Crossbeam sender. It
/// intentionally limits the surface of the sender. Much of this
/// is just boilerplate wrapping
pub struct Sender<T: MachineImpl> {
    channel_id: usize,
    connection: ThreadSafeConnection,
    pub sender: crossbeam::channel::Sender<T>,
    receiver_machine: WeakShareableMachine,
}

impl<T> Sender<T>
where
    T: MachineImpl,
{
    pub fn get_id(&self) -> usize { self.channel_id }
    pub fn bind(&mut self, recevier_machine: WeakShareableMachine) { self.receiver_machine = recevier_machine; }
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> { self.sender.try_send(msg) }

    pub fn sender(&self) -> crossbeam::channel::Sender<T> { self.sender.clone() }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>>
    where
        T: MachineImpl + MachineImpl<InstructionSet = T> + std::fmt::Debug,
    {
        // Blocking on send is a pretty ugly process; the first block is allowed to complete,
        // as if it had not blocked. However, the machine state becomes SendBlock. If a subsequent
        // send is attempted while SendBlock'd, the executor will pause until the prior send completes.

        // if already SendBlock'd this won't return until the send completes
        ExecutorData::block_or_continue();
        // now, that the sender is not blocked, it should be running, try to complete the send
        // and schedule the receiver if needed
        match self.sender.try_send(msg) {
            Ok(()) => {
                if let Some(machine) = self.receiver_machine.upgrade() {
                    // log::debug!(
                    // "chan {} send machine {} state {:#?} q_len {}",
                    // self.channel_id,
                    // machine.get_key(),
                    // machine.get_state(),
                    // self.sender.len()
                    // );
                    match machine.compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready) {
                        Ok(_) => ExecutorData::schedule(&machine, false),
                        // new machines will be scheduled when assigned, scheduling here would be bad
                        Err(MachineState::New) => (),
                        // already running is perfection
                        Err(MachineState::Running) => (),
                        // ready should already be scheduled
                        Err(MachineState::Ready) => (),
                        // send block will clear and schedule
                        Err(MachineState::SendBlock) => (),
                        // anything else we need to decide what to do, so log it
                        Err(state) => {
                            log::error!("chan {} state {:#?} q_len {}", self.channel_id, state, self.sender.len());
                        },
                    }
                }
                Ok(())
            },
            Err(TrySendError::Full(instruction)) => {
                if let Some(machine) = self.receiver_machine.upgrade() {
                    log::trace!(
                        "parking sender {} with cmd {:#?} machine {} state {:#?}",
                        self.channel_id,
                        instruction,
                        machine.get_key(),
                        machine.get_state()
                    );
                }
                match <T as MachineImpl>::park_sender(
                    self.channel_id,
                    Weak::clone(&self.receiver_machine),
                    self.sender.clone() as crossbeam::channel::Sender<<T as MachineImpl>::InstructionSet>,
                    instruction,
                ) {
                    // parked, the machine should now have a state of SendBlock
                    Ok(()) => Ok(()),
                    // not parked ,due to it being the main thread, just send and let main block
                    Err(m) => {
                        log::debug!("blocking main thread on send");
                        self.sender.send(m)
                    },
                }
            },
            // on disconnect, return to the caller with an error
            Err(TrySendError::Disconnected(m)) => Err(SendError(m)),
        }
    }

    pub fn send_timeout(&self, msg: T, timeout: std::time::Duration) -> Result<(), SendTimeoutError<T>> {
        if self.sender.is_full() {
            log::warn!("Sender: channel is full, send_timeout will block for {:#?}", timeout);
        }
        self.sender.send_timeout(msg, timeout)
    }

    pub fn is_full(&self) -> bool { self.sender.is_full() }

    pub fn len(&self) -> usize { self.sender.len() }

    pub fn is_empty(&self) -> bool { self.sender.is_empty() }

    pub fn capacity(&self) -> Option<usize> { self.sender.capacity() }
}

impl<T> fmt::Debug for Sender<T>
where
    T: MachineImpl,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.sender.fmt(f) }
}

impl<T> Drop for Sender<T>
where
    T: MachineImpl,
{
    fn drop(&mut self) {
        if let Some(machine) = self.receiver_machine.upgrade() {
            if Arc::strong_count(&self.connection) == 1 {
                log::trace!(
                    "dropping sender {} machine {} state {:#?}",
                    self.channel_id,
                    machine.get_key(),
                    machine.get_state()
                );
                machine.set_disconnected();
                match machine.compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready) {
                    Ok(_) => ExecutorData::schedule(&machine, true),
                    Err(MachineState::New) => log::info!("dropping sender, while machine is new"),
                    Err(MachineState::Running) => log::info!(
                        "dropping sender {} machine {} state {:#?}, not sched",
                        self.channel_id,
                        machine.get_key(),
                        MachineState::Running,
                    ),
                    Err(MachineState::Ready) => log::info!(
                        "dropping sender {} machine {} state {:#?}, not sched",
                        self.channel_id,
                        machine.get_key(),
                        MachineState::Ready,
                    ),
                    Err(state) => {
                        log::info!(
                            "droping sender {} machine {} state {:#?} q_len {}",
                            self.channel_id,
                            machine.get_key(),
                            state,
                            self.sender.len()
                        );
                    },
                }
            }
        }
    }
}

impl<T> Clone for Sender<T>
where
    T: MachineImpl,
{
    fn clone(&self) -> Self {
        Self {
            channel_id: self.channel_id,
            connection: Arc::clone(&self.connection),
            sender: self.sender.clone(),
            receiver_machine: Weak::clone(&self.receiver_machine),
        }
    }
}

impl<T> PartialEq for Sender<T>
where
    T: MachineImpl,
{
    fn eq(&self, other: &Self) -> bool { self.channel_id == other.channel_id && self.sender.same_channel(&other.sender) }
}

impl<T> Eq for Sender<T> where T: MachineImpl {}

pub fn wrap_sender<T>(sender: crossbeam::channel::Sender<T>, channel_id: usize, connection: ThreadSafeConnection) -> Sender<T>
where
    T: MachineImpl,
{
    log::trace!("creating sender {}", channel_id);
    Sender::<T> {
        channel_id,
        connection,
        sender,
        receiver_machine: Weak::new(),
    }
}
