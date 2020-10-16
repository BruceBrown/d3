//! As the name implies, d3-core is the core foundation for the server. It is responsible for creating
//! a d3 machine from a raw implementation, scheduling that machine to run and running it. Creating a
//! machine is trivial, as the exmples illustate.
//!
//! # examples
//! ```
//! # use std::sync::{Arc, Mutex};
//! # #[allow(unused_imports)]
//! # use d3_core::executor;
//! # use d3_core::machine_impl::*;
//! # use d3_core::send_cmd;
//! # use d3_derive::*;
//! # #[derive(Debug, MachineImpl)]
//! # pub enum TestMessage {Test,}
//! #
//! // A simple Alice machine
//! struct Alice {}
//! // Alice implements the TestMessage instruction set
//! impl Machine<TestMessage> for Alice {
//!   fn receive(&self, _message: TestMessage) {}
//! }
//! # executor::set_selector_maintenance_duration(std::time::Duration::from_millis(20));
//! # executor::start_server();
//! // Construct Alice, and connect her
//! let (alice, sender) = executor::connect(Alice{});
//! // the returned alice is an Arc<Mutex<Alice>>
//! // the returned sender is a Sender<TestMessage>
//! // At this point, Alice is in the scheduler, but idle.
//! // sending a command will wake her and call her receive method.
//! send_cmd(&sender, TestMessage::Test);
//! # executor::stop_server();
//! ```
//! Occasionally, a machine may implement multiple instruction sets. This
//! is an example of Alice implementing TestMessage and StateTable.
//! ```
//! # use std::sync::{Arc, Mutex};
//! # #[allow(unused_imports)]
//! # use d3_core::executor;
//! # use d3_core::machine_impl::*;
//! # use d3_core::send_cmd;
//! # use d3_derive::*;
//! # #[derive(Debug, MachineImpl)]
//! # pub enum TestMessage {Test,}
//! # #[derive(Debug, MachineImpl)]
//! # pub enum StateTable {Start,}
//! #
//! // A simple Alice machine
//! struct Alice {}
//! // Alice implements the TestMessage instruction set
//! impl Machine<TestMessage> for Alice {
//!   fn receive(&self, _message: TestMessage) {}
//! }
//! // Alice also implements the StateTable instruction set
//! impl Machine<StateTable> for Alice {
//!   fn receive(&self, _message: StateTable) {}
//! }
//! # executor::set_selector_maintenance_duration(std::time::Duration::from_millis(20));
//! # executor::start_server();
//! // Construct Alice, and connect her, this is for the TextMessage set
//! let (alice, sender) = executor::connect::<_,TestMessage>(Alice{});
//! // the returned alice is an Arc<Mutex<Alice>>
//! // the returned sender is a Sender<TestMessage>
//! // At this point, Alice is in the scheduler, but idle.
//! // Now add the StateTable instruction set
//! let state_sender = executor::and_connect::<_,StateTable>(&alice);
//! // the returned sender is a Sender<StateTable>
//! // At this point, there are two Alice machines is in the scheduler, both idle.
//! // Both machines share the Alice implementation, each with their own
//! // sender and receiver.
//! // sending a command will wake her and call her receive method.
//! send_cmd(&sender, TestMessage::Test);
//! send_cmd(&state_sender, StateTable::Start);
//! # executor::stop_server();
//! ```
//!
//! Instruction are varients in an enum, the enum is considered an insruction set. A MachineImpl
//! derive macro is used to convert an enum into an instruction set.
//!
//! # examples
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use d3_core::machine_impl::*;
//! use d3_derive::*;
//! #[derive(Debug, MachineImpl)]
//! pub enum StateTable {
//!     Init,
//!     Start,
//!     Stop
//! }
//! // The MachineImpl provides all of the code necessary to support
//! // a machine sending and receiving StateTable instructions.
//! ```
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
    pub mod simple_event_timer;
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
    mod connection;
    pub mod machine_channel;
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
    use crate::foundation::{machine::*, simple_event_timer::*};
    use crate::tls::{collective::*, tls_executor::*};
    pub mod executor;
    pub mod machine;
    mod overwatch;
    pub mod sched;
    mod sched_factory;
    pub mod setup_teardown;
    pub mod traits;
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
        collective::{CollectiveState, MachineAdapter, MachineDependentAdapter, MachineState, ShareableMachine},
        tls_executor::{
            tls_executor_data, CollectiveSenderAdapter, ExecutorDataField, ExecutorStats,
            SharedCollectiveSenderAdapter, Task, TrySendError,
        },
    };
}

// package up the send_cmd utility function
pub use crate::collective::machine::send_cmd;

// package up thing needed to tune and start the schduler and inject
// machines into the collective.
pub mod executor {
    pub use crate::scheduler::{
        executor::{get_time_slice, set_time_slice},
        machine::{
            and_connect, and_connect_unbounded, and_connect_with_capacity, connect, connect_unbounded,
            connect_with_capacity, get_default_channel_capacity, set_default_channel_capacity,
        },
        sched::{
            get_machine_count, get_machine_count_estimate, get_selector_maintenance_duration,
            set_machine_count_estimate, set_selector_maintenance_duration,
        },
        setup_teardown::{get_executor_count, set_executor_count, start_server, stop_server},
    };
    pub mod stats {
        pub use crate::scheduler::{
            sched::SchedStats,
            setup_teardown::{add_core_stats_sender, remove_core_stats_sender},
            traits::{CoreStatsMessage, CoreStatsSender},
        };
        pub use crate::tls::tls_executor::ExecutorStats;
    }
}
