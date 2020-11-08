# Rust Core Runtime for D3 -- A Framework for Server Development

[![Build Status](https://github.com/BruceBrown/d3/workflows/Rust/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![Test Status](https://github.com/BruceBrown/d3/workflows/Tests/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](
https://github.com/BruceBrown/d3#license)
[![Cargo](https://img.shields.io/crates/v/d3-core.svg)](
https://crates.io/crates/d3-core)
[![Documentation](https://docs.rs/d3-core/badge.svg)](
https://docs.rs/d3-core)
[![Rust 1.47+](https://img.shields.io/badge/rust-1.47+-color.svg)](
https://www.rust-lang.org)

The core runtime for the d3 framework. d3-core is a companion to d3-derive and d3-components.
Combined, they form a framework for server development.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
d3-derive = "0.1.3"
d3-core = "0.1.3"
```

## Example
```rust
#[macro_use]
extern crate d3_derive;

use d3_core::machine_impl::*;
use d3_core::executor;

// A trivial instruction set
#[derive(Debug, MachineImpl)]
enum StateTable { Init, Start, Stop }

// A trivial Alice
pub struct Alice {}

// Implement the Machine trait for Alice
impl Machine<StateTable> for Alice {
     fn receive(&self, cmd: StateTable) {
     }
}

// Start the scheduler and executor.
executor::start_server();

// create the Machine from Alice, getting back a machine and Sender<StateTable>.
let (alice, sender) = executor::connect(Alice{});

// send a command to Alice
// Alice's receive method will be invoked, with cmd of StateTable::Init.
sender.send(StateTable::Init).expect("send failed");

// Stop the scheduler and executor.
executor::stop_server();
```
