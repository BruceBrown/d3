//! The d3-dev-instruction-sets library provides an examples of instruction sets.
//! The TestMessage instruction set is the most complete, and implemented by the
//! Forwarder. The Forwarder is used in integration and bench tests.
#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;

// this should become a prelude
#[allow(unused_imports)] use std::sync::Arc;

use d3::core::machine_impl::*;

#[allow(unused_imports)] use d3::d3_derive::*;

mod test_message;
pub use test_message::{ChaosMonkeyMutation, TestMessage, TestStruct};

mod something;
pub use something::Something;

mod state_table;
pub use state_table::StateTable;
