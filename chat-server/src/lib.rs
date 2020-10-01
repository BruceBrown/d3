
#[allow(unused_imports)]
#[macro_use] extern crate smart_default;
extern crate crossbeam;

use std::sync::{Arc, Mutex};
use std::error::Error;

#[allow(unused_imports)]
use d3_lib::{executor};
use d3_lib::machine_impl::*;
use d3_components::*;
use d3_components::settings::{self, SimpleConfig, Service};
use d3_components::components::{ComponentCmd, ComponentSender, AnySender};

mod chat_instruction_set;
use chat_instruction_set::{ChatCmd, Data, ChatSender};

pub mod chat_coordinator;
pub mod chat_producer;
pub mod chat_consumer;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
