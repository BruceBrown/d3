
#[macro_use] extern crate smart_default;
#[macro_use] extern crate serde_derive;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::convert::TryInto;

use atomic_refcell::AtomicRefCell;
use crossbeam::atomic::AtomicCell;

#[allow(unused_imports)]
use d3_core::executor::*;
use d3_core::machine_impl::*;
use d3_derive::*;

mod net_instruction_set;
use net_instruction_set::{NetSender, NetCmd};

pub mod settings;
pub mod components;
pub use components::send_cmd;
pub mod coordinators;
mod mio_network;

mod network_start_stop;
use network_start_stop::{NetworkFactory, NetworkFactoryObj, NetworkControl, NetworkControlObj};
pub mod network {
    pub use crate::net_instruction_set::{NetCmd, NetSender};
    pub use crate::network_start_stop::{start_network, stop_network, get_network_sender};
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
