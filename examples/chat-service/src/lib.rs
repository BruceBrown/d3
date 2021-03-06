//! A Chat Server. All joiners are connected with each other.
//!
//! This crate illustrates how to use the d3 component/coordinator model,
//! along with d3 core to implement a server, albeit a somewhat trivial
//! server. It illustrates:
//! * Adding a TCP Listener and handling connections.
//! * Creating Components and Instances.
//! * Adding an instruction set and using `#[derive(MachineImpl)]`
//! * Interacting with the network component via the `NetCmd` instruction set.
//!
//! The server is configured via settings and its only public interfaces are:
//! * chat_coordinator::configure()
//! * chat_producer::configure()
//! * chat_consumer::configure()
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
        settings::{self, Component, Coordinator, CoordinatorVariant, Service, Settings, SimpleConfig},
        *,
    },
    core::{
        executor::{self},
        machine_impl::*,
        *,
    },
    d3_derive::*,
};

mod chat_instruction_set;
use chat_instruction_set::{ChatCmd, ChatSender, Data};

pub mod chat_consumer;
pub mod chat_coordinator;
pub mod chat_producer;

#[cfg(test)]
mod tests {
    use super::*;
    use simplelog::*;
    use std::collections::HashMap;

    #[test]
    fn chat_server() {
        // This illustrates how the chat server is configured and fits into the server model.
        //

        // install a simple logger
        CombinedLogger::init(vec![TermLogger::new(LevelFilter::Error, Config::default(), TerminalMode::Mixed)]).unwrap();

        // start the server and network
        executor::start_server();
        network::start_network();

        let mut components: Vec<ComponentInfo> = Vec::new();
        let mut settings = settings::Settings::default();

        // configure the consumer
        let config = SimpleConfig {
            enabled: true,
            ..SimpleConfig::default()
        };
        if let Ok(Some(sender)) = chat_consumer::configure(config, &settings) {
            components.push(ComponentInfo::new("ChatConsumer".to_string(), sender));
        } else {
            assert_eq!(true, false);
        }

        // configure the producer
        let config = SimpleConfig {
            enabled: true,
            ..SimpleConfig::default()
        };
        if let Ok(Some(sender)) = chat_producer::configure(config, &settings) {
            components.push(ComponentInfo::new("ChatProducer".to_string(), sender));
        } else {
            assert_eq!(true, false);
        }

        // now for the more complex configuring of the coordinator
        let mut kv: HashMap<String, String> = HashMap::new();
        kv.insert("name_prompt".to_string(), "Welcome, ur name?".to_string());
        let mut chat_map: HashMap<settings::Coordinator, CoordinatorVariant> = HashMap::new();
        chat_map.insert(
            "ChatCoordinator".to_string(),
            CoordinatorVariant::SimpleTcpConfig {
                tcp_address: "127.0.0.1:7000".to_string(),
                kv: Some(kv),
            },
        );
        settings.coordinator.push(chat_map);
        settings.services.insert("ChatService".to_string());
        if let Ok(Some(coordinator)) = chat_coordinator::configure(&settings, components.as_slice()) {
            // no longer need components
            drop(components);
            // start the server
            coordinator.send(ComponentCmd::Start).unwrap();
            // for the next few moments you can connect to the server at 127.0.0.1:7000
            std::thread::sleep(std::time::Duration::from_millis(100));
            // stop the server
            coordinator.send(ComponentCmd::Stop).unwrap();
        } else {
            assert_eq!(true, false);
        }
        // finally, shut down the network and server
        network::stop_network();
        executor::stop_server();
    }
}
