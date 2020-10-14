#[macro_use] extern crate smart_default;

use crossbeam::atomic::AtomicCell;
use std::net::SocketAddr;
use uuid::Uuid;

use d3_components::settings::*;
use d3_components::{components::*, network::*};
use d3_core::executor;
use d3_core::machine_impl::*;

pub mod alice;
