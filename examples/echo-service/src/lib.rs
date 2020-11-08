#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;
extern crate crossbeam;

use parking_lot::Mutex;
use std::convert::TryInto;
use std::sync::Arc;

// Maybe turn this into a prelude?
#[allow(unused_imports)]
use d3::{
    self,
    components::{
        self,
        network::{self, *},
        settings::{self, Coordinator, CoordinatorVariant, Service, Settings, SimpleConfig},
        *,
    },
    core::{
        executor::{self},
        machine_impl::*,
        *,
    },
    d3_derive::*,
};

mod echo_instruction_set;
use echo_instruction_set::{EchoCmd, EchoCmdSender, EchoData};

pub mod echo_consumer;
pub mod echo_coordinator;
pub mod echo_producer;
