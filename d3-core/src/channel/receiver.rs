use super::*;
use crossbeam::{TryRecvError, RecvError, RecvTimeoutError};
use self::{connection::*};

/// Wrap the crossbeam::Sender so that we warn on block, In the future we might
/// park and continue later.Receiver
pub struct Receiver<T>
where T: MachineImpl
{
    channel_id: usize,
    connection: ThreadSafeConnection,
    pub receiver: crossbeam::Receiver<T>,
}

impl<T> Receiver<T>
where T: MachineImpl
{
    pub fn clone_receiver(&self) -> crossbeam::Receiver<T> {
        self.receiver.clone()
    }
    pub fn receiver(&self) -> &crossbeam::Receiver<T> {
        &self.receiver
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        self.receiver.recv()
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration,) -> Result<T, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.receiver.is_full()
    }

    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.receiver.capacity()
    }

    pub fn iter(&self) -> crossbeam::Iter<'_, T> {
        self.receiver.iter()
    }

    pub fn try_iter(&self) -> crossbeam::TryIter<'_, T> {
        self.receiver.try_iter()
    }
}

impl<T> Clone for Receiver<T>
where T: MachineImpl
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
where T: MachineImpl
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.receiver.fmt(f)
    }
}

impl<T> PartialEq for Receiver<T>
where T: MachineImpl
{
    fn eq(&self, other: &Self) -> bool {
        self.channel_id == other.channel_id && self.receiver.same_channel(&other.receiver)
    }
}

impl<T> Eq for Receiver<T> where T: MachineImpl {}

pub fn wrap_receiver<T>(
    receiver: crossbeam::Receiver<T>,
    channel_id: usize,
    connection: ThreadSafeConnection,
) -> Receiver<T>
where T: MachineImpl
{
    Receiver::<T> {
        channel_id,
        connection,
        receiver,
    }
}
/*
impl<T> ChannelHandle for Receiver<T>
where T: MachineImpl + Clone
{
    fn add_select<'a,'b>(&'a self, select: &mut crossbeam::Select<'b>) -> usize
    where 'a: 'b,
    {
        select.recv(&self.receiver)
    }
}
*/