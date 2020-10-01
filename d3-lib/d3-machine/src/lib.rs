
#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;


// this should become a prelude
#[allow(unused_imports)]
use std::sync::{Arc,Mutex};

mod machine;
mod thread_safe;

pub use machine::{MachineImpl, Machine};
pub use thread_safe::{ProtectedInner, SharedProtectedObject};


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
