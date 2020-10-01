
#[allow(unused_imports)]

#[macro_use] extern crate smart_default;
use std::cell::RefCell;

use d3_machine::{SharedProtectedObject};

mod tls_executor;

pub use tls_executor::{CollectiveState, MachineState, ExecutorState, TrySendError};
pub use tls_executor::{SharedCollectiveSenderAdapter, CollectiveSenderAdapter, ExecutorData, tls_executor_data};
pub use tls_executor::{ExecutorDataField, ExecutorNotifier, ExecutorNotifierObj };

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
