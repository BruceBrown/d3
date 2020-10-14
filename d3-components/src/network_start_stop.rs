#![allow(dead_code)]
use super::*;

use crate::mio_network::new_network_factory;
///
/// While much of d3 is focussed on data drive dispatch of machines, we can't ignore the fact that
/// we're going to have to deal with networks. Afterall, the whole point of doing this is to build
/// a server model, and what good is a server if it can't interact with the outside world. So, here
/// we go...
///
/// Mio provides the low-level notifications that the network needs servicing. We're still going to
/// rely upon machines to perform the services. we're going to use mio as a signalling layer.
/// Much like how we use Crossbeam select to schedule machines, we're going to use Mio polling
/// to do the same for those few machines which facilitate communicating with the outside world.
///
/// Mostly, we're porting the example. Instead of main, we'll launch a thread. We're going to forego
/// logging, as I've got an idea for that were we have a very thin UDP logging to something else.
/// The idea being you're goingto be in a docker kind of world, where our server should focus
/// on handling data, and another, local server, should focus on logging and yet anotehr should focus
/// on metrics.
///

#[allow(non_upper_case_globals)]
static network_state: AtomicCell<NetworkState> = AtomicCell::new(NetworkState::Stopped);

#[allow(non_upper_case_globals)]
static network: AtomicRefCell<Network> = AtomicRefCell::new(Network {
    network_control: NetworkField::Uninitialized,
    network_sender: NetworkField::Uninitialized,
});

/// This is the network state
#[derive(Debug, Copy, Clone, Eq, PartialEq, SmartDefault)]
enum NetworkState {
    #[default]
    Stopped,
    Initializing,
    Stopping,
    Running,
}

#[derive(SmartDefault)]
enum NetworkField {
    #[default]
    Uninitialized,
    Network(NetworkControlObj),
    NetworkSender(NetSender),
}

/// The network
#[derive(SmartDefault)]
pub struct Network {
    network_control: NetworkField,
    network_sender: NetworkField,
}
impl Network {
    /// get the network sender
    pub fn get_sender() -> NetSender {
        match &network.borrow().network_sender {
            NetworkField::NetworkSender(network_sender) => network_sender.clone(),
            _ => panic!("network is not running, unable to get sender."),
        }
    }
}

#[inline]
pub fn get_network_sender() -> NetSender { Network::get_sender() }

/// start the network
pub fn start_network() {
    network_state.store(NetworkState::Initializing);
    let network_factory = new_network_factory();
    let network_sender = network_factory.get_sender();
    let network_control = network_factory.start();
    let mut s = network.borrow_mut();
    s.network_control = NetworkField::Network(network_control);
    s.network_sender = NetworkField::NetworkSender(network_sender);
    network_state.store(NetworkState::Running);
}

/// stop the network
pub fn stop_network() {
    network_state.store(NetworkState::Stopping);
    if let NetworkField::Network(network_control) = &network.borrow().network_control {
        network_control.stop()
    }

    let mut s = network.borrow_mut();
    s.network_control = NetworkField::Uninitialized;
    s.network_sender = NetworkField::Uninitialized;
    network_state.store(NetworkState::Stopped);
}

// The factory for the network
pub trait NetworkFactory {
    /// get a clone of the sender for the network
    fn get_sender(&self) -> NetSender;
    /// start the network
    fn start(&self) -> NetworkControlObj;
}
pub type NetworkFactoryObj = Arc<dyn NetworkFactory>;

// The controller for the system monitor.
pub trait NetworkControl: Send + Sync {
    /// stop the network
    fn stop(&self);
}
pub type NetworkControlObj = Arc<dyn NetworkControl>;
