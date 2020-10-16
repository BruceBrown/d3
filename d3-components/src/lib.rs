//! Beyond the pure running of machines, a structure should be imposed upon them, allowing for interaction.
//! Many different organizational structures exist, which could be utilzed. This is one of them, it is a
//! Component, Connector model with a Coordinator Component providing orchestration. Although this is the
//! model being discussed there is no requirement that it be the only model used. It is one I happen to
//! like and provides some nice characteristics.
//!
//! The primary benefit of this model is that it reduces most things down to a wiring problem, which
//! works nicely if you can accomplish everything by wiring senders together. The other benefit is
//! the incredibly small API which needs to be exposed. Ideally, it consists of a configure per
//! component.
//!
//! There are two examples provided, that illustrate this:
//! * A chat server, which allows all joiners to chat with each other via TCP.
//! * An echo server, which echos back anything sent to it.
//!
//! Components may interact with the network as well as other components. They are factories for
//! connections. Connections are the work-horses of machines. They may spawn additional machines
//! as part of their function. They provide scaling of the server. The Coordinator is a Component
//! with some added responsiblities. It is responsible for wiring Connectors together in a meaningful
//! manner.
//!
//! Taken together, this model can be used for implementing a simple echo server all the way up to a
//! more complex conference server, which would include audio, video, recording and translating services.
#[macro_use] extern crate smart_default;
#[macro_use] extern crate serde_derive;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use atomic_refcell::AtomicRefCell;
use crossbeam::atomic::AtomicCell;

#[allow(unused_imports)] use d3_core::executor::*;
use d3_core::machine_impl::*;
use d3_core::send_cmd;
use d3_derive::*;

mod net_instruction_set;
use net_instruction_set::{NetCmd, NetConnId, NetSender};

pub mod components;
pub mod coordinators;
mod mio_network;
pub mod settings;

mod network_start_stop;
use network_start_stop::{NetworkControl, NetworkControlObj, NetworkFactory, NetworkFactoryObj};
pub mod network {
    pub use crate::net_instruction_set::{NetCmd, NetConnId, NetSender};
    pub use crate::network_start_stop::{get_network_sender, start_network, stop_network};
}
