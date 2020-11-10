use self::connection::*;
use super::*;
use crossbeam::channel::{RecvError, RecvTimeoutError, TryRecvError};

/// The Receiver is a wrapper aruond the Crossbeam receiver. It
/// intentionally limits the surface of the receiver. Much of this
/// is just boilerplate wrapping
pub struct Receiver<T>
where
    T: MachineImpl,
{
    channel_id: usize,
    connection: ThreadSafeConnection,
    pub receiver: crossbeam::channel::Receiver<T>,
}

impl<T> Receiver<T>
where
    T: MachineImpl,
{
    pub fn get_id(&self) -> usize { self.channel_id }
    pub fn clone_receiver(&self) -> crossbeam::channel::Receiver<T> { self.receiver.clone() }
    pub fn receiver(&self) -> &crossbeam::channel::Receiver<T> { &self.receiver }

    pub fn try_recv(&self) -> Result<T, TryRecvError> { self.receiver.try_recv() }

    pub fn recv(&self) -> Result<T, RecvError> { self.receiver.recv() }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> { self.receiver.recv_timeout(timeout) }

    pub fn is_empty(&self) -> bool { self.receiver.is_empty() }

    pub fn is_full(&self) -> bool { self.receiver.is_full() }

    pub fn len(&self) -> usize { self.receiver.len() }

    pub fn capacity(&self) -> Option<usize> { self.receiver.capacity() }

    pub fn iter(&self) -> crossbeam::channel::Iter<'_, T> { self.receiver.iter() }

    pub fn try_iter(&self) -> crossbeam::channel::TryIter<'_, T> { self.receiver.try_iter() }
}

impl<T> Clone for Receiver<T>
where
    T: MachineImpl,
{
    fn clone(&self) -> Self {
        Self {
            channel_id: self.channel_id,
            connection: self.connection.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

impl<T> fmt::Debug for Receiver<T>
where
    T: MachineImpl,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.receiver.fmt(f) }
}

impl<T> PartialEq for Receiver<T>
where
    T: MachineImpl,
{
    fn eq(&self, other: &Self) -> bool { self.channel_id == other.channel_id && self.receiver.same_channel(&other.receiver) }
}

impl<T> Eq for Receiver<T> where T: MachineImpl {}

impl<T> Drop for Receiver<T>
where
    T: MachineImpl,
{
    // while there can be any numbers of senders, by design, there can only be one receiver per channel
    fn drop(&mut self) {
        log::debug!("Receiver: dropped receiver {}", self.get_id());
    }
}

pub fn wrap_receiver<T>(receiver: crossbeam::channel::Receiver<T>, channel_id: usize, connection: ThreadSafeConnection) -> Receiver<T>
where
    T: MachineImpl,
{
    Receiver::<T> {
        channel_id,
        connection,
        receiver,
    }
}
