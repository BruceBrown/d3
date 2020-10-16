#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;
extern crate crossbeam;

use std::convert::TryInto;
use std::sync::{Arc, Mutex};

use uuid::{self};

#[allow(unused_imports)] use d3_core::executor::{self};
use d3_core::machine_impl::*;
use d3_core::send_cmd;
use d3_derive::*;

use d3_components::components::{AnySender, ComponentCmd, ComponentError, ComponentSender};
use d3_components::settings::{self, Service, SimpleConfig};
use d3_components::*;
mod echo_instruction_set;
use echo_instruction_set::{EchoCmd, EchoCmdSender, EchoData};

mod consumer;
mod echo_coordinator;
mod producer;

pub mod coordinator {
    pub mod echo_service {
        pub use crate::echo_coordinator::configure;
    }
}
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

    #[test]
    fn echo_server_60() {}
}
