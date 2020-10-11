// pull in commonly used elements
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::iter;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

// bring in addition utilities
use atomic_refcell::AtomicRefCell;
use crossbeam::atomic::AtomicCell;
use uuid::Uuid;

#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;

#[allow(unused_imports)]
#[macro_use]
extern crate log;

// pull in all of the modules
mod foundation {
    use super::*;
    pub mod machine;
    pub mod thread_safe;
}
mod tls {
    #![allow(dead_code)]
    use super::*;
    use crate::foundation::thread_safe::*;
    pub mod collective;
    pub mod tls_executor;
}
mod channel {
    #![allow(dead_code)]
    use super::*;
    use crate::foundation::machine::*;
    use crate::tls::tls_executor::ExecutorData;
    pub mod machine_channel;
    mod connection;
    pub mod receiver;
    pub mod sender;
}
mod collective {
    #![allow(dead_code)]
    use super::*;
    use crate::channel::{receiver::*, sender::*};
    use crate::foundation::machine::*;
    use crate::tls::collective::*;
    pub mod machine;
}
mod scheduler {
    #![allow(dead_code)]
    use super::*;
    use crate::collective::machine::*;
    use crate::foundation::machine::*;
    use crate::tls::{collective::*, tls_executor::*};
    pub mod executor;
    pub mod machine;
    mod overwatch;
    pub mod sched;
    mod sched_factory;
    pub mod setup_teardown;
    mod traits;
}

// publish the parts needed outside of the core

// package up things needed for #[derive(MachineImpl)]
pub mod machine_impl {
    pub use crate::channel::{
        machine_channel::{channel, channel_with_capacity},
        receiver::Receiver,
        sender::Sender,
    };
    pub use crate::collective::machine::MachineBuilder;
    pub use crate::foundation::{machine::Machine, machine::MachineImpl};
    pub use crate::tls::{
        collective::{
            CollectiveState, MachineAdapter, MachineDependentAdapter, MachineState,
            ShareableMachine,
        },
        tls_executor::{
            tls_executor_data, CollectiveSenderAdapter, ExecutorDataField, ExecutorStats,
            SharedCollectiveSenderAdapter, Task, TrySendError,
        },
    };
}

// package up thing needed to tune and start the schduler and inject
// machines into the collective.
pub mod executor {
    pub use crate::scheduler::{
        machine::{
            and_connect, and_connect_unbounded, and_connect_with_capacity, connect,
            connect_unbounded, connect_with_capacity, get_default_channel_capacity,
            set_default_channel_capacity,
        },
        sched::{
            get_machine_count_estimate, get_selector_maintenance_duration,
            set_machine_count_estimate, set_selector_maintenance_duration,
            get_machine_count,
        },
        executor::{get_time_slice, set_time_slice,
        },
        setup_teardown::{get_executor_count, set_executor_count, start_server, stop_server},
    };
}
