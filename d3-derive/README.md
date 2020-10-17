# Rust Derive Macro for D3 -- A Framework for Server Development

[![Build Status](https://github.com/BruceBrown/d3/workflows/Rust/badge.svg)](
https://github.com/brucebrown/d3/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](
https://github.com/BruceBrown/d3#license)
[![Rust 1.47+](https://img.shields.io/badge/rust-1.47+-color.svg)](
https://www.rust-lang.org)

Custom derive for automatically implementing the `MachineImpl` trait for an enum, tranforming it into a d3 instruction set. d3-derive is a companion to d3-core and d3-components. Combined, they form a framework for server development.

```rust
#[macro_use]
extern crate d3_derive;

#[derive(MachineImpl)]
pub enum Foo {
    Bar,
    Baz {
        name: String,
    },
    Baa (u32),
}
```
