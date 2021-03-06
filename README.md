# D3 -- A Framework for Server Development

[![Build Status](https://github.com/BruceBrown/d3/workflows/Rust/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![Test Status](https://github.com/BruceBrown/d3/workflows/Tests/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](
https://github.com/BruceBrown/d3#license)
[![Cargo](https://img.shields.io/crates/v/d3.svg)](
https://crates.io/crates/d3)
[![Documentation](https://docs.rs/d3/badge.svg)](
https://docs.rs/d3)
[![Rust 1.47+](https://img.shields.io/badge/rust-1.47+-color.svg)](
https://www.rust-lang.org)

### Recent Performance Improvements Achieve 10M Cmd/Sec
|Machine Specs | |
|---------:|-----------------|
|MacOS | Calalina Version 10.15.7 |
|Processor | 1.1Ghz Dual Core Intel Core i3 |
|Memory | 16Gb 2722 Hz LPDDR4X |
|bench test| Daisy-chain of 4000 machines, sending 200 commands (800K cmds) in 79.412ms as measured by Criterion |

#### Similar results are shown in github workflow with Linux:
|Machine Specs | |
|---------:|-----------------|
|Ubuntu | 20.04.1 LTS
|Processor | 2-core CPU
|Memory | 7 GB of RAM memory
|bench test| Daisy-chain of 4000 machines, sending 200 commands  (800K cmds) in 81.994 as measured by Criterion.

## D3
This crate provides a framework for server development. It is especially
well suited for those cases where the server employs a pipeline architecture.

There are two core concepts, the machine, and the instruction set. Combined
with a channel sender and receiver, you have all of the parts necessary for
building a server. Strictly speaking, the d3 framework can be used for non-server
projects, anywhere where you have concurrent, cooperative object instances.


## The Instruction Set
The Instruction Set starts with any kind of enum. It becomes an instruction set
when `MachineImpl` is derived.

## The Machine
The machine is an instance of any struct, implementing one or more instruction sets
and connected to the collective. Machines, asynchronously, communicate with each
other via the instruction sets they implement.

## Example
This example shows how easy it is to create an instruction set, create a machine
and send an instruction to that machine.
``` rust
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

// create the Machine from Alice, getting back a machine and Sender<StateTable>.
let (alice, sender) = executor::connect(Alice{});

// send a command to Alice
// Alice's receive method will be invoked, with cmd of StateTable::Init.
sender.send(StateTable::Init).expect("send failed");
```

## Crates
The main `d3` crate just [re-exports](src/lib.rs) tools from smaller subcrates:
### d3-derive
* [`MachineImpl`](d3-derive), a derive macro for tranforming an enum into a d3 instruction set.
### d3-core
* [`machine_impl`](d3-core), a packaged namespace to be used alongside <quote>#[derive(MachineImpl)]</quote>.
* [`executor`](d3-core), a packaged namespace to facilitate interacting with the collective.
### d3-components
* [`components`](d3-components), a packaged namespace for managing machines. It is modeled upon a component, coordinator, connector model.
* [`network`](d3-components), a TCP abstraction consumable by machines. It wraps Mio.

## Examples
A numer of aexamples are available. Fork the d3 repo and `cargo run -p test-server --release` for an example of a server with services.

### Services
* [`echo service`](examples/echo-service), an example of a TCP echo service
* [`udp echo service`](examples/udp-echo-service), an example of a UDP echo service
* [`chat service`](examples/chat-service), an example of a TCP chat service
* [`alice service`](examples/alice-service), an example of an HTTP service with form manipulation
* [`monitor service`](examples/monitor-service), an example of how to monitor the core

### Applications
* [`test server`](examples/test-server), an example of a configurable server providing services
* [`Conway's Game of Life`](examples/d3-life), an example of a game with a UI and interacting machines

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
d3 = "0.1.3"
```

## Compatibility

d3 supports stable Rust releases going back at least six months,
and every time the minimum supported Rust version is increased, a new minor
version is released. Currently, the minimum supported Rust version is 1.47.

## Contributing

d3 welcomes contribution from everyone in the form of suggestions, bug reports,
pull requests, and feedback. 💛

If you need ideas for contribution, there are several ways to get started:
* Found a bug or have a feature request?
[Submit an issue](https://github.com/brucebrown/d3/issues/new)!
* Issues and PRs labeled with
[feedback wanted](https://github.com/brucebrown/d3/issues?utf8=%E2%9C%93&q=is%3Aopen+sort%3Aupdated-desc+label%3A%22feedback+wanted%22+)
* Issues labeled with
  [good first issue](https://github.com/brucebrown/d3/issues?q=is%3Aissue+is%3Aopen+sort%3Aupdated-desc+label%3A%22good+first+issue%22)
  are relatively easy starter issues.

## Learning Resources

If you'd like to learn more read our [wiki](https://github.com/brucebrown/d3/wiki)

## Conduct

The d3 project adheres to the
[Rust Code of Conduct](https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md).
This describes the minimum behavior expected from all contributors.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.


## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
