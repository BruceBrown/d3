use super::*;

///
/// Wrap the crossbeam sender to allow the executor to handle a block.
/// This requires that a send which would block, parks the send. It
/// also requires that prior to sending a check is made to determine
/// if the sender is already blocked. The one thing that makes this
/// work is that the repeat send is bound to the executor. So,
/// we can examine TLS data to see if we need to not complete the send.
///
pub struct Sender<T: MachineImpl>
{
    channel_id: usize,
    connection: ThreadSafeConnection,
    pub sender: crossbeam::Sender<T>,
}

impl<T> Sender<T>
where T: MachineImpl
{
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(msg)
    }

    pub fn sender(&self) -> crossbeam::Sender<T> {
        self.sender.clone()
    }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>>
    where T: MachineImpl + MachineImpl<InstructionSet = T>
    {
        match self.sender.try_send(msg) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(instruction)) => {
                match <T as MachineImpl>::park_sender(
                    self.sender.clone() as crossbeam::Sender<<T as MachineImpl>::InstructionSet>,
                    instruction) {
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

    pub fn send_timeout(
        &self,
        msg: T,
        timeout: std::time::Duration,
    ) -> Result<(), SendTimeoutError<T>> {
        if self.sender.is_full() {
            log::warn!("Sender: channel is full, send_timeout will block for {:#?}", timeout);
        }
        self.sender.send_timeout(msg, timeout)
    }

    pub fn is_full(&self) -> bool {
        self.sender.is_full()
    }

    pub fn len(&self) -> usize {
        self.sender.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.sender.capacity()
    }
}

impl<T> fmt::Debug for Sender<T>
where T: MachineImpl
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.sender.fmt(f)
    }
}

impl<T> Clone for Sender<T>
where T: MachineImpl,
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
where T: MachineImpl
{
    fn eq(&self, other: &Self) -> bool {
        self.channel_id == other.channel_id && self.sender.same_channel(&other.sender)
    }
}

impl<T> Eq for Sender<T> where T: MachineImpl, {}

pub fn wrap_sender<T>(
    sender: crossbeam::Sender<T>,
    channel_id: usize,
    connection: ThreadSafeConnection,
) -> Sender<T>
where T: MachineImpl,
{   Sender::<T> {
        channel_id,
        connection,
        sender,
    }
}
/*
impl<T> ChannelHandle for Sender<T>
where T: 'static + MachineImpl + Clone,
{
    fn add_select<'a,'b>(&'a self, select: &mut crossbeam::Select<'b>) -> usize
    where 'a: 'b,
    {
        select.send(&self.sender)
    }
}
*/
