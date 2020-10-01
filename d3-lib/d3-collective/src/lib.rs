
// consider building up a prelude
use std::sync::{Arc,Mutex};
use std::time::Duration;

use crossbeam::atomic::AtomicCell;

#[allow(unused_imports)]
use d3_channel::{Sender, Receiver, channel_with_capacity};
use d3_tls::*;
use d3_machine::{MachineImpl, Machine};


// bring in the modules
mod collective;
#[allow(unused_imports)]
use collective::*;

mod machine;
#[allow(unused_imports)]
use machine::*;

// publish
pub use collective::{SharedCollectiveAdapter, CollectiveAdapter, CommonCollectiveAdapter};
pub use collective::{ExecutorStats, Task};
pub use collective::{get_time_slice, set_time_slice, CHANNEL_MAX, COLLECTIVE_ID};

pub use machine::{MachineAdapter};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
