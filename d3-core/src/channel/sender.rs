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
}

impl<T> Sender<T>
where
    T: MachineImpl,
{
    pub fn get_id(&self) -> usize { self.channel_id }
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> { self.sender.try_send(msg) }

    pub fn sender(&self) -> crossbeam::channel::Sender<T> { self.sender.clone() }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>>
    where
        T: MachineImpl + MachineImpl<InstructionSet = T> + std::fmt::Debug,
    {
        // this could be a series of sends from a machine, which is already
        // blocked. Need to check if that is the case, otherwise this send
        // could succeed and we'd be sending out of order.
        ExecutorData::block_or_continue();
        match self.sender.try_send(msg) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(instruction)) => {
                log::debug!("parking sender {} with cmd {:#?}", self.channel_id, instruction);
                match <T as MachineImpl>::park_sender(
                    self.channel_id,
                    self.sender.clone() as crossbeam::channel::Sender<<T as MachineImpl>::InstructionSet>,
                    instruction,
                ) {
                    Ok(()) => Ok(()),
                    Err(m) => {
                        log::warn!("blocking main thread on send");
                        self.sender.send(m)
                    },
                }
            },
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

impl<T> Clone for Sender<T>
where
    T: MachineImpl,
{
    fn clone(&self) -> Self {
        Self {
            channel_id: self.channel_id,
            connection: self.connection.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<T> PartialEq for Sender<T>
where
    T: MachineImpl,
{
    fn eq(&self, other: &Self) -> bool {
        self.channel_id == other.channel_id && self.sender.same_channel(&other.sender)
    }
}

impl<T> Eq for Sender<T> where T: MachineImpl {}

pub fn wrap_sender<T>(
    sender: crossbeam::channel::Sender<T>,
    channel_id: usize,
    connection: ThreadSafeConnection,
) -> Sender<T>
where
    T: MachineImpl,
{
    Sender::<T> {
        channel_id,
        connection,
        sender,
    }
}
