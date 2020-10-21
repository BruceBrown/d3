use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

// Maybe turn this into a prelude?
#[allow(unused_imports)]
use d3::{
    self,
    components::{
        self,
        network::*,
        settings::{self, Coordinator, CoordinatorVariant, Service, Settings, SimpleConfig},
        *,
    },
    core::{
        executor::{self, stats::*, *},
        machine_impl::*,
        *,
    },
    d3_derive::*,
};
pub mod monitor;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
