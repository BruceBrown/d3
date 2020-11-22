//! A Framework for Server Development
//! This crate provides a set of tools, for implementing a server. It is especially
//! well suited for those cases where the server employs a pipeline architecture.
//! There are two core concepts, the machine, and the instruction set. Combined
//! with a channel sender and receiver, you have all of the parts necessary for
//! building a server.
//!
//! ## The Machine
//! The Machine starts with any kind of struct. It becomes a machine by implementing
//! the Machine trait for an instruction set and joining the collective. There's only
//! one method that requires implementing. Joining the collective is a single call
//! into the core, which returns a wrapped machine and a sender for the instruction
//! set. The machine is the receiver for any instruction sent to that sender. In
//! most cases the wrapped instance can be ignored.
//!
//! ## The Instruction Set
//! The Instruction Set starts with any kind of enum. It becomes an instruction set
//! when MachineImpl is derived.
//!
//! ## Example
//!
//! This example shows how easy it is to create an instruction set, create a machine
//! and send an instruction to that machine.
//! ``` text
//! // A trivial instruction set
//! #[derive(Debug, MachineImpl)]
//! enum StateTable { Init, Start, Stop }
//!
//! pub struct Alice {}
//! // implement the Machine trait for Alice
//! impl Machine<StateTable> for Alice {
//!     fn receive(&self, cmd: StateTable) {
//! }
//!
//! // create the Machine from Alice, getting back a machine and Sender<StateTable>.
//! let (alice, sender) = executor::connect(Alice{});
//!
//! // send a command to Alice
//! // Alice's receive method will be called, likely on a different thread than this thread.
//! sender.send(StateTable::Init).expect("send failed");
//! ```
//! The main `d3` crate just re-exports tools from smaller subrates:
//! ## Derive Macro
//!
//! * [`d3-derive`], a derive macro for transforming an enum into an instruction set.
//!
//! ## Core
//!
//! * [`d3-core`], a sceduler and executor for machines.
//!
//!
//! ## Components
//!
//! * [`d3-components`], provides a component/coordinator heirachy of machines.
//!
//!
//! ## Instruction Sets and Test Drivers
//!
//! * [`d3-dev-instruction-sets`](https://github.com/BruceBrown/d3/tree/master/d3-dev-instruction-sets/src),
//! example of some simple instruction sets.
//! * [`d3-test-driver`](https://github.com/BruceBrown/d3/tree/master/d3-test-drivers/src),
//!  example of implementing an instruction set. The test driver
//! is used by bench and test.
//!
//! ## Examples
//!
//! ### Services
//! * [`alice-service`](https://github.com/BruceBrown/d3/tree/master/examples/alice-service/src/alice.rs),
//! an example of a web-service sending a form and processing the POST.
//! * [`chat-service`](https://github.com/BruceBrown/d3/tree/master/examples/chat-service/src),
//! an example of a trivial chat service.
//! * [`echo-service`](https://github.com/BruceBrown/d3/tree/master/examples/echo-service/src),
//! an example of a trivial echo service.
//! * [`monitor-service`](https://github.com/BruceBrown/d3/tree/master/examples/monitor-service/src),
//! an example of service for monitoring the core.
//!
//! ### Server
//! * [`test-server`](https://github.com/BruceBrown/d3/tree/master/examples/test-server/src),
//! an example of a server running the aforementioned services

// re-publish all the bits, so that you only need d3.
pub mod d3_derive {
    pub use d3_derive::*;
}
pub mod core {
    pub mod machine_impl {
        pub use d3_core::machine_impl::{self, *};
    }
    pub mod executor {
        pub use d3_core::executor::{self, *};
    }
    pub use d3_core::send_cmd;
}
pub mod components {
    pub mod network {
        pub use d3_components::network::{self, *};
    }
    pub use d3_components::components::{self, *};
    pub use d3_components::coordinators::{self, *};
    pub mod settings {
        pub use d3_components::settings::{self, *};
    }
}
pub mod conference {
    pub use conference;
}