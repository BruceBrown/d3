
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam::{SendError, SendTimeoutError, TrySendError};
use crossbeam::{RecvError, RecvTimeoutError, TryRecvError};


use d3_machine::{MachineImpl};

mod channel;
mod connection;
mod receiver;
mod sender;

pub use crate::channel::{channel, channel_with_capacity};
pub use crate::connection::{Connection, ThreadSafeConnection};
pub use crate::receiver::Receiver;
pub use crate::sender::Sender;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
