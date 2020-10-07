
#[allow(unused_imports)]
#[macro_use] extern crate smart_default;
extern crate crossbeam;

use std::sync::{Arc, Mutex};
use std::error::Error;

use uuid::{self, Uuid};

#[allow(unused_imports)]
use d3_core::executor::{self};
use d3_core::machine_impl::*;
use d3_derive::*;

use d3_components::*;
use d3_components::settings::{self, SimpleConfig, Service};
use d3_components::components::{ComponentCmd, ComponentSender, AnySender, send_cmd};
mod echo_instruction_set;
use echo_instruction_set::{EchoCmd, EchoData, EchoCmdSender};

mod echo_coordinator;
mod producer;
mod consumer;

pub mod coordinator {
    pub mod echo_server {
        pub use crate::echo_coordinator::configure;
}}
pub mod component {
    pub mod echo_consumer {
        pub use crate::consumer::configure;
    }
    pub mod echo_producer {
        pub use crate::producer::configure;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn echo_server_60() {
    }
}
