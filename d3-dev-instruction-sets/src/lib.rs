#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;

// this should become a prelude
#[allow(unused_imports)] use std::sync::{Arc, Mutex};

use d3_core::machine_impl::*;

#[allow(unused_imports)] use d3_derive::*;

// The instruction sets don't need to live here. However,
// its good to have 1 for testing and a 2nd to ensure that
// more that one can be handled concurrently.
//
// Consider having an instruction_set mod, which all get published into.

mod something;
pub use something::Something;

mod state_table;
pub use state_table::StateTable;

mod test_message;
pub use test_message::{ChaosMonkeyMutation, TestMessage, TestStruct};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
