//! A Framework for Server Development
//! What we have here is a Framework, for implementing a server. It is especially
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
//!
//! ## Core
//!
//! * [`d3-core`], a sceduler and executor for machines.
//! * [`d3-derive`], a derive macro for transforming an enum into an instruction set.
//!
//! ## Components
//!
//! * [`d3-components`], provides a component/coordinator heirachy of machines.
//!
//! ## Services
//!
//! * [`echo-service`], an example of a trivial echo service.
//! * [`chat-service`], an example of a trivial chat service.
//!
//! ## Instruction Sets and Test Drivers
//!
//! * [`d3-dev-instruction-sets`], example of some simple instruction sets.
//! * [`d3-test-driver`], example of implementing an instruction set. The test driver is used by
//! bench and test.
//!
//! ## Examples
//!
//! * [`monitor-service`], an example of monitoring the core.
//! * [`alice-service`], an example of a web-server sending a form and processing the POST.
