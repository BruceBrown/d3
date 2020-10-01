#![allow(dead_code)]
#[allow(unused_imports)]
use super::*;


/// This is where the high level concepts being. The model I've settled on, at least
/// for now is a Coordinator, Component, and Connector model. The component is a
/// factory for a connection. The Coordinator is responsible for interacting with
/// components and assembling connections in a meaningful way. Let's take a simple
/// thing like an echo server. You connect to it and anything you send gets echoed
/// back. Instead of the simplest of implementations, ours will have 3 connectors
/// per connection. The first, an echo_coordinator is responsible for receiving bytes
/// from the network and sending bytes to the network connection. In turn, the
/// coordinator sends to an echo_consumer, which forwards to a echo_producer, which
/// forwards back to our coordinator. That whole mess involves 6 machines, and each
/// new connection adds another 3 machines. However, if we want to change to a chat
/// server, all it takes is having the consumer forward to multiple producers and
/// your done. A very small change with a bigger impact.
///

/// Sometimes you need a brute force solution, at least until something better shows
/// up. This is the brute force solution for dealing will components. We're going
/// to have what essentially is a list of component initializers. They're each
/// going to be invoked and given a hunk of the setting and each will determine 
/// if they are active or inert. Leaving us with a set of active components.
/// 

/// The list of components, we're going to use a vector, which we just add to. Each
/// implements the Component trait.




///this will move if I can figure out way to hide the sender.
#[derive(Debug, MachineImpl)]
pub enum ComponentCmd {
    /// Some components don't need to be told to start, others do
    Start,
    /// An orderly shutdown is preferred
    Stop,
    /// A new session announcement, the sender is where new instances should rendezvous
    /// (conn_id, service, sender )
    NewSession(u128, settings::Service, Arc<dyn std::any::Any + Send + Sync>),
}
pub type AnySender = Arc<dyn std::any::Any + Send + Sync>;
pub type ComponentSender = Sender<ComponentCmd>;

#[derive(Debug)]
pub enum ComponentError {
    NotEnabled(String),
    BadConfig(String),
}

///
/// ComponentInfo describes an active component. In the future this might contain
/// which services this component provides support for.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    /// Which component this is
    pub component: settings::Component,
    /// The sender for the component
    pub sender: ComponentSender,
}


// A utility function that can be use when sending
#[inline]
pub fn send_cmd<T>(sender: &Sender<T>, cmd: T)
where T: MachineImpl + MachineImpl<InstructionSet = T>,
{
    match sender.send(cmd) {
        Ok(_) => (),
        Err(e) => log::info!("failed to send instruction: {}", e), /* should do some logging */
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use d3_lib::instruction_sets::TestMessage;

    #[test]
    fn test_destination_sender() {

        struct Controller {
        }
        impl Machine<ComponentCmd> for Controller {
            fn receive(&self, cmd: ComponentCmd) {
                match cmd {
                    ComponentCmd::NewSession(conn_id, service, pkg) => {
                        if let Ok(sender) = Arc::clone(&pkg).downcast::<Sender<ComponentCmd>>() {
                            println!("got a component sender");
                        } else if let Ok(sender) = Arc::clone(&pkg).downcast::<Sender<TestMessage>>() {
                            println!("got a test sender");
                        }
                    },
                    _ => (),
                }
            }
        }

        impl Machine<TestMessage> for Controller {
            fn receive(&self, cmd: TestMessage) {
            }
        }

        executor::start_server();
        let (m, component_sender) = executor::connect::<_,ComponentCmd>(Controller{});
        let test_sender = executor::and_connect::<_,TestMessage>(&m);
        let pkg = Arc::new(component_sender.clone());
        component_sender.send(ComponentCmd::NewSession(1234, settings::Service::ChatServer, pkg));
        thread::sleep(std::time::Duration::from_millis(50));

        executor::stop_server();
    }
}
