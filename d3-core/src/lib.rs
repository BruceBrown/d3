
// pull in commonly used elements
use std::sync::{Arc, Weak, Mutex, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::thread;
use std::collections::HashMap;
use std::iter;
use std::fmt;
use std::cell::RefCell;


// bring in addition utilities
use crossbeam::{atomic::AtomicCell};
use atomic_refcell::AtomicRefCell;
use uuid::Uuid;

#[allow(unused_imports)]
#[macro_use] extern crate smart_default;

#[allow(unused_imports)]
#[macro_use]extern crate log;

// pull in all of the modules
mod machine {
    use super::*;
    pub mod machine;
    pub mod thread_safe;
}
mod tls {
    #![allow(dead_code)]
    use super::*;
    use crate::machine::{thread_safe::*};
    pub mod tls_executor;
    pub mod collective;
}
mod channel {
    #![allow(dead_code)]
    use super::*;
    use crate::machine::{machine::*};
    use crate::tls::tls_executor::ExecutorData;
    pub mod channel;
    mod connection;
    pub mod receiver;
    pub mod sender;
}
mod collective {
    #![allow(dead_code)]
    use super::*;
    use crate::machine::{machine::*};
    use crate::tls::{tls_executor::*, collective::*};
    use crate::channel::{receiver::*, sender::*};
    pub mod collective;
    pub mod machine;
}
mod scheduler {
    #![allow(dead_code)]
    use super::*;
    use crate::machine::{machine::*};
    use crate::tls::{tls_executor::*, collective::*};
    use crate::collective::{collective::*, machine::*};
    mod executor;
    pub mod machine;
    mod overwatch;
    mod sched;
    mod sched_factory;
    pub mod setup_teardown;
    mod traits;
}

// publish the parts needed outside of the core

// package up things needed for #[derive(MachineImpl)]
pub mod machine_impl {
    pub use crate::channel::{channel::{channel, channel_with_capacity}, receiver::Receiver, sender::Sender};
    pub use crate::collective::{ 
        collective::{ get_time_slice },
        machine::{ MachineBuilder }
    };
    pub use crate::tls::{ collective:: {
        MachineDependentAdapter, ShareableMachine, MachineAdapter,
        MachineState, CollectiveState,
        },
        tls_executor:: {
            ExecutorDataField,
            ExecutorStats, Task,TrySendError, tls_executor_data,CollectiveSenderAdapter, SharedCollectiveSenderAdapter
        }
    };
    pub use crate::machine::{machine::Machine, machine::MachineImpl};
}

// package up thing needed to tune and start the schduler and inject
// machines into the collective.
pub mod executor {
    pub use crate::scheduler::{machine::{
        connect, connect_unbounded, connect_with_capacity,
        and_connect, and_connect_unbounded, and_connect_with_capacity,
        get_default_channel_capacity, set_default_channel_capacity,

    }, setup_teardown::{
        start_server, stop_server,
        get_executor_count, set_executor_count
    }};
    pub use crate::collective::{ collective::{get_time_slice, set_time_slice,}};
}


#[cfg(test)]
use super::*;
mod tests {
    #[test]
    fn it_works() {}
}
