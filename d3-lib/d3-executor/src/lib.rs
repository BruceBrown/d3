#[allow(unused_imports)]
#[macro_use]extern crate smart_default;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration,Instant};

use crossbeam::atomic::AtomicCell;
use crossbeam::deque;

use atomic_refcell::AtomicRefCell;


#[allow(unused_imports)]
use d3_collective::*;

#[allow(unused_imports)]
use d3_tls::*;

mod traits;
use traits::*;

mod overwatch;
use overwatch::SystemMonitorFactory;

mod scheduler;
use scheduler::new_scheduler_factory;

mod executor;
use executor::SystemExecutorFactory;

mod machine;
mod setup_teardown;

pub use machine::{connect, connect_unbounded, connect_with_capacity};
pub use machine::{and_connect};
pub use machine::{get_default_channel_capacity, set_default_channel_capacity};
pub use setup_teardown::{get_executor_count, set_executor_count};
pub use setup_teardown::{start_server, stop_server};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
