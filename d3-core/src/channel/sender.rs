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
    clone_count: Arc<AtomicUsize>,
    connection: ThreadSafeConnection,
    pub sender: crossbeam::channel::Sender<T>,
    receiver_machine: ShareableMachine,
}

impl<T> Sender<T>
where
    T: MachineImpl,
{
    pub fn get_id(&self) -> usize { self.channel_id }
    pub fn bind(&mut self, recevier_machine: ShareableMachine) { self.receiver_machine = recevier_machine; }
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
                match self
                    .receiver_machine
                    .compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready)
                {
                    Ok(_) => ExecutorData::schedule(Arc::clone(&self.receiver_machine)),
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
                Ok(())
            },
            Err(TrySendError::Full(instruction)) => {
                log::trace!(
                    "parking sender {} with cmd {:#?} machine {} state {:#?}",
                    self.channel_id,
                    instruction,
                    self.receiver_machine.get_key(),
                    self.receiver_machine.get_state()
                );

                match <T as MachineImpl>::park_sender(
                    self.channel_id,
                    Arc::clone(&self.receiver_machine),
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
        if 0 != self.clone_count.fetch_sub(1, Ordering::SeqCst) {
            return;
        }

        self.receiver_machine.set_disconnected();
        match self
            .receiver_machine
            .compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready)
        {
            Ok(_) => {
                ExecutorData::schedule(Arc::clone(&self.receiver_machine));
            },
            Err(MachineState::New) => (),
            Err(MachineState::Ready) => (),
            Err(MachineState::Running) => (),
            Err(MachineState::Dead) => (),
            Err(state) => panic!("sender drop not expecting receiver state {:#?}", state),
        }
    }
}

impl<T> Clone for Sender<T>
where
    T: MachineImpl,
{
    fn clone(&self) -> Self {
        self.clone_count.fetch_add(1, Ordering::SeqCst);
        Self {
            channel_id: self.channel_id,
            clone_count: Arc::clone(&self.clone_count),
            connection: Arc::clone(&self.connection),
            sender: self.sender.clone(),
            receiver_machine: Arc::clone(&self.receiver_machine),
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

impl<T> PartialOrd for Sender<T>
where
    T: MachineImpl,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.channel_id.cmp(&other.channel_id)) }
}

impl<T> Ord for Sender<T>
where
    T: MachineImpl,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.channel_id.cmp(&other.channel_id) }
}

pub fn wrap_sender<T>(sender: crossbeam::channel::Sender<T>, channel_id: usize, connection: ThreadSafeConnection) -> Sender<T>
where
    T: MachineImpl,
{
    log::trace!("creating sender {}", channel_id);
    Sender::<T> {
        channel_id,
        clone_count: Arc::new(AtomicUsize::new(0)),
        connection,
        sender,
        receiver_machine: Arc::new(MachineAdapter::default()),
    }
}
