#![allow(dead_code)]
#[allow(unused_imports)] use super::*;

/// ComponentCmd is the instruction set for components. It provides a means of
/// starting, and stopping a component. Additionally, it signals when a service
/// has a new session along with the sender for the coordinator of the session.
#[derive(Debug, MachineImpl)]
pub enum ComponentCmd {
    /// Starts a component, some components don't need to be told to start, others do.
    /// Start is sent soon after the server is started and is automatic. It notifies
    /// the component that it can complete any deferred setup and should be in a
    /// running state.
    Start,
    /// Stops a component. Stop is sent as the server is shutting down and
    /// is automatic.It notifies the component that the server is about to stop
    /// and that the component should cleanup anything it needs to cleanup before
    /// the server stops.
    Stop,
    /// NewSession announces that there's a new session which other components
    /// may want to know about. The tupple is a session Uuid, a service type,
    /// and an anonymous sender. Presumably, the component responding to this
    /// is able to convert the sender to a sender which it can interact with.
    NewSession(u128, settings::Service, Arc<dyn std::any::Any + Send + Sync>),
}

/// Shorthand for an anonymous sender, that can be sent instructions from a specific
/// instruction set. The following example illustrates how to convert an AnySender
/// to a ComponentSender.
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # use std::sync::Arc;
/// # use crossbeam;
/// # use crossbeam::channel::{Sender,Receiver};
/// # type AnySender = Arc<dyn std::any::Any + Send + Sync>;
/// # enum ComponentCmd { Start, Stop }
/// # type ComponentSender = Sender<ComponentCmd>;
/// # enum StateTable { Start, Stop }
/// # fn channel<T>() -> (crossbeam::channel::Sender<T>, crossbeam::channel::Receiver<T>) {
/// #   crossbeam::channel::unbounded()
/// # }
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #
/// /// Consume `sender` returning either `Some(sender: ComponentSender)` or 'None'.
/// fn as_component_sender(any_sender: AnySender) -> Option<ComponentSender>
/// {
///     if let Ok(sender) = any_sender.downcast::<ComponentSender>() {
///         // pull out the sender and clone it, as its not copyable
///         Some((*sender).clone())
///     } else {
///         None
///     }
/// }
/// let (sender, receiver) = channel::<ComponentCmd>();
/// let any_sender: AnySender = Arc::new(sender);
/// assert_eq!(as_component_sender(any_sender).is_some(), true);
///
/// let (sender, receiver) = channel::<StateTable>();
/// let any_sender: AnySender = Arc::new(sender);
/// assert_eq!(as_component_sender(any_sender).is_some(), false);
///
/// # Ok(())
/// }
/// ```
pub type AnySender = Arc<dyn std::any::Any + Send + Sync>;
/// Shorthand for a sender, that can be sent ComponentCmd instructions.
pub type ComponentSender = Sender<ComponentCmd>;

/// ComponentError describes the types of errors that a component may return.
/// This is most used during configuration.
#[derive(Debug)]
pub enum ComponentError {
    /// Indicates that a component was unable to fully activate itself.
    NotEnabled(String),
    /// Indicates that the component was unable to obtain
    /// enough configuration information to fully activate itself.
    BadConfig(String),
}

/// ComponentInfo describes an active component. It provides the component type
/// and the sender for the component. It is assembled during the config startup phase
/// and sent to each coordinator.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    component: settings::Component,
    sender: ComponentSender,
}
impl ComponentInfo {
    /// Creates and new ComponentInfo struct and returns it.
    pub const fn new(component: settings::Component, sender: ComponentSender) -> Self { Self { component, sender } }
    /// Get the component type for this component
    pub const fn component(&self) -> &settings::Component { &self.component }
    /// Get a reference to the sender for this component
    pub const fn sender(&self) -> &ComponentSender { &self.sender }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)] use super::*;
    use simplelog::*;

    use d3_core::executor;

    #[derive(Debug, MachineImpl)]
    pub enum TestMessage {
        Test,
    }

    #[test]
    fn test_new_session() {
        // tests that we can have an instruction with any sender as:
        // Arc<dyn std::any::Any + Send + Sync> and convert back to
        // the correct sender. This is critical as we use the service
        // to indicate the type of sender, and only components which
        // understand the service should attempt to participate by
        // decoding the sender and responding appropriately.
        #[derive(Default)]
        struct Controller {
            counter: AtomicCell<usize>,
        }
        impl Machine<ComponentCmd> for Controller {
            fn receive(&self, cmd: ComponentCmd) {
                match cmd {
                    ComponentCmd::NewSession(conn_id, service, sender) => {
                        assert_eq!(conn_id, 12345);
                        assert_eq!(service, "EchoServer".to_string());
                        assert_eq!(false, Arc::clone(&sender).downcast::<Sender<TestMessage>>().is_ok());
                        assert_eq!(true, Arc::clone(&sender).downcast::<Sender<ComponentCmd>>().is_ok());
                        self.counter.store(1);
                    },
                    _ => assert_eq!(true, false),
                }
            }
        }
        impl Machine<TestMessage> for Controller {
            fn receive(&self, _cmd: TestMessage) {
                assert_eq!(true, false);
            }
        }
        // install a simple logger
        CombinedLogger::init(vec![TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed)]).unwrap();
        executor::start_server();
        thread::sleep(std::time::Duration::from_millis(20));
        let (m, component_sender) = executor::connect::<_, ComponentCmd>(Controller::default());
        let _test_sender = executor::and_connect::<_, TestMessage>(&m);

        if component_sender
            .send(ComponentCmd::NewSession(
                12345,
                "EchoServer".to_string(),
                Arc::new(component_sender.clone()),
            ))
            .is_err()
        {}
        thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(m.counter.load(), 1);
        executor::stop_server();
    }
}
