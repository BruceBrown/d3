pub use d3_derive::*;

//
// The scheduler can take advantage of a local modification to select.
// the mod returns a vector of ready operations. This is controlled
// by #[cfg(select_multiple)]
//

// package up the MachineImpl into a module
pub mod machine_impl {
    pub use d3_channel::{channel, channel_with_capacity, Receiver, Sender};
    pub use d3_collective::{CollectiveAdapter, CommonCollectiveAdapter};
    pub use d3_collective::{ExecutorStats, Task};
    pub use d3_collective::{MachineAdapter, SharedCollectiveAdapter};
    pub use d3_collective::{COLLECTIVE_ID, get_time_slice};
    pub use d3_derive::*;
    pub use d3_machine::{Machine, MachineImpl};
    pub use d3_tls::TrySendError;
    pub use d3_tls::{tls_executor_data, CollectiveState, MachineState};
    pub use d3_tls::{CollectiveSenderAdapter, SharedCollectiveSenderAdapter};
}

pub mod executor {
    pub use d3_executor::{
        connect, connect_unbounded, connect_with_capacity,
        and_connect,
        start_server, stop_server,
    };
    pub use d3_executor::{get_default_channel_capacity, set_default_channel_capacity};
    pub use d3_executor::{get_executor_count, set_executor_count};
    pub use d3_collective::{get_time_slice, set_time_slice};
}

pub mod instruction_sets {
    pub use d3_instruction_sets::{TestMessage, TestStruct};
}

#[cfg(test)]
use super::*;
mod tests {
    #[test]
    fn it_works() {}
}
