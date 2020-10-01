use super::*;

use crate::connection::Connection;
use crate::receiver::wrap_receiver;
use crate::sender::wrap_sender;

static CHANNEL_ID: AtomicUsize = AtomicUsize::new(1);

// channel construction is limited to using only instruction sets that
// implement MachineImpl. 

/// Create a channel with a fixed capacity.
pub fn channel_with_capacity<T>(capacity: usize) -> (Sender<T>, Receiver<T>)
where T: MachineImpl
{
    let (s, r) = crossbeam::bounded(capacity);
    wrap(s,r)
}

/// Create a channel with an unlimited capacity. This should be
/// used with caution, as it can cause a panic when sending.
pub fn channel<T>() -> (Sender<T>, Receiver<T>)
where T: MachineImpl
{
    let (s, r) = crossbeam::unbounded();
    wrap(s, r)
}

fn wrap<T>(sender: crossbeam::Sender<T>, receiver: crossbeam::Receiver<T>) -> (Sender<T>, Receiver<T>)
where T: MachineImpl
{
    let channel_id = CHANNEL_ID.fetch_add(1, Ordering::SeqCst);
    let (sc, rc) = Connection::new();
    (
        wrap_sender(sender, channel_id, sc),
        wrap_receiver(receiver, channel_id, rc),
    )
}
