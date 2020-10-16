use self::{connection::*, receiver::*, sender::*};
use super::*;

/// This is a small wrapper around the crossbeam channel. There are a
/// few reasons for this. We may want to use adifferent channel
/// implementation in the future, so we want to encapsulate it.
/// We want to take action when sending to a channel that is full,
/// otherwise we block a thread, forcing the spawning of a thread.
/// We want to enforce that the channels are limited to using
/// only types that derive from MachineImpl.

/// The channel id can be used in logging, otherwise its useless
static CHANNEL_ID: AtomicUsize = AtomicUsize::new(1);

/// Create a channel with a fixed capacity.
pub fn channel_with_capacity<T>(capacity: usize) -> (Sender<T>, Receiver<T>)
where
    T: MachineImpl,
{
    let (s, r) = crossbeam::bounded(capacity);
    wrap(s, r)
}

/// Create a channel with an unlimited capacity. This should be
/// used with caution, as it can cause a panic when sending.
pub fn channel<T>() -> (Sender<T>, Receiver<T>)
where
    T: MachineImpl,
{
    let (s, r) = crossbeam::unbounded();
    wrap(s, r)
}

fn wrap<T>(sender: crossbeam::Sender<T>, receiver: crossbeam::Receiver<T>) -> (Sender<T>, Receiver<T>)
where
    T: MachineImpl,
{
    let channel_id = CHANNEL_ID.fetch_add(1, Ordering::SeqCst);
    let (sc, rc) = Connection::new();
    (
        wrap_sender(sender, channel_id, sc),
        wrap_receiver(receiver, channel_id, rc),
    )
}
