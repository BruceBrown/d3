#![allow(dead_code)]
use super::*;

use crate::mio_network::new_network_factory;

/// Internally, an AtomicCell<NetworkState>, is used for determining the state
/// of the network. This used by start_network(), stop_network(). An
/// AtomicRefCell<Network> is used to access the network struct. Both are operating
/// as singletons, which I'm really not happy with.
#[allow(non_upper_case_globals)]
static network_state: AtomicCell<NetworkState> = AtomicCell::new(NetworkState::Stopped);

#[allow(non_upper_case_globals)]
static network: AtomicRefCell<Network> = AtomicRefCell::new(Network {
    network_control: NetworkField::Uninitialized,
    network_sender: NetworkField::Uninitialized,
});

/// The NetworkState describes each of the states the network can be in
#[derive(Debug, Copy, Clone, Eq, PartialEq, SmartDefault)]
enum NetworkState {
    #[default]
    Stopped,
    Initializing,
    Stopping,
    Running,
}

/// NetworkFields are used to allow a default Network struct, the fields
/// provide an Uninitialized variant, along with a varient for each field
/// in the Network struct that can't otherwise be defaulted.
#[derive(SmartDefault)]
enum NetworkField {
    #[default]
    Uninitialized,
    Network(NetworkControlObj),
    NetworkSender(NetSender),
}

/// The Network struct contains fields used by start_network(), stop_network(),
/// and get_network_sender().
#[derive(SmartDefault)]
struct Network {
    network_control: NetworkField,
    network_sender: NetworkField,
}
impl Network {
    /// get the network sender
    fn get_sender() -> NetSender {
        match &network.borrow().network_sender {
            NetworkField::NetworkSender(network_sender) => network_sender.clone(),
            _ => panic!("network is not running, unable to get sender."),
        }
    }
}

#[inline]
/// Obtain the network's sender. The returned sender is a clone,
/// that you are free to use, further clone, or drop. Tip: cache
/// the result and clone it when you need to send it.
pub fn get_network_sender() -> NetSender { Network::get_sender() }

/// Start the network. Starting does not perform any network bindings, it
/// prepares the network to accept bindings.
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

/// Stop the network. Stopping will close all network connections and
/// listeners and no further network activity is allowed.
pub fn stop_network() {
    network_state.store(NetworkState::Stopping);
    if let NetworkField::Network(network_control) = &(network.borrow()).network_control {
        network_control.stop()
    }

    let mut s = network.borrow_mut();
    s.network_control = NetworkField::Uninitialized;
    s.network_sender = NetworkField::Uninitialized;
    network_state.store(NetworkState::Stopped);
    // give machines a chance to react to network shutdown
    thread::sleep(Duration::from_millis(100));
}

/// The factory for the network
pub trait NetworkFactory {
    /// get a clone of the sender for the network
    fn get_sender(&self) -> NetSender;
    /// start the network
    fn start(&self) -> NetworkControlObj;
}
/// The trait object for the network factory
pub type NetworkFactoryObj = Arc<dyn NetworkFactory>;

// The controller for the network.
pub trait NetworkControl: Send + Sync {
    /// stop the network
    fn stop(&self);
}
/// The trait object for the network controller
pub type NetworkControlObj = Arc<dyn NetworkControl>;
