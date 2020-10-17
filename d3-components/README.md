# Rust Component Layer for D3 -- A Framework for Server Development

[![Build Status](https://github.com/BruceBrown/d3/workflows/Rust/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](
https://github.com/BruceBrown/d3#license)
[![Rust 1.47+](https://img.shields.io/badge/rust-1.47+-color.svg)](
https://www.rust-lang.org)

The components layer provides an organization hierarchy for machines.
It is based upon a Component/Coordinator/Connector model, and while not the only possible model, it is one I like.
This layer is where the network is exposed. It is an adapter wrapping Mio.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
d3-derive = "0.1.0"
d3-core = "0.1.0"
d3-components = "0.1.0"
```


## Example Listening on an address:port
```rust
#[macro_use]
extern crate d3_derive;

use d3_core::machine_impl::*;
use d3_core::executor;
use d3_components::network;

// A trivial Alice
pub struct Alice {}

// Implement the Machine trait for Alice
impl Machine<network::NetCmd> for Alice {
     fn receive(&self, cmd: StateTable) {
     }
}

// Start the scheduler and executor and network
executor::start_server();
network::start_network();

// create the Machine from Alice, getting back a machine and Sender<StateTable>.
let (alice, sender) = executor::connect(Alice{});

// send a command to the network asking for Alice to be notified if a connection
// is received for 127.0.0.1:4000
network::get_network_sender().send(NetCmd::BindListener("127.0.0.1:4000".to_string, sender)).expect("send failed");

// Stop the scheduler and executor and network
network::start_network();
executor::stop_server();
```