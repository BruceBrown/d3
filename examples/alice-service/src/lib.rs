#[macro_use] extern crate smart_default;

use crossbeam::atomic::AtomicCell;
use std::net::SocketAddr;
use uuid::Uuid;

// Maybe turn this into a prelude?
#[allow(unused_imports)]
use d3::{
    self,
    components::{
        self,
        network::*,
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

pub mod alice;
