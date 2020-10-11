#![allow(dead_code)]
#[allow(unused_imports)]
use super::*;

/// ComponentCmd is the instruction set for components. It provides a means of
/// starting, and stopping a component. Additionally, it signals when a service
/// has a new session along with the sender for the coordinator of the session.
#[derive(Debug, MachineImpl)]
pub enum ComponentCmd {
    // Starts a component, some components don't need to be told to start, others do
    Start,
    // Stops a component
    Stop,
    // A new session announcement, the sender is where new instances should rendezvous
    // (conn_id, service, sender )
    NewSession(
        u128,
        settings::Service,
        Arc<dyn std::any::Any + Send + Sync>,
    ),
}
pub type AnySender = Arc<dyn std::any::Any + Send + Sync>;
pub type ComponentSender = Sender<ComponentCmd>;

/// ComponentError describes the types of errors that a component may return.
/// This is most used during configuration.
#[derive(Debug)]
pub enum ComponentError {
    NotEnabled(String),
    BadConfig(String),
}

/// ComponentInfo describes an active component. It provides the component type
/// and the sender for the component.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    component: settings::Component,
    sender: ComponentSender,
}
impl ComponentInfo {
    pub fn new(component: settings::Component, sender: ComponentSender) -> Self {
        Self { component, sender }
    }
    pub fn component(&self) -> settings::Component {
        self.component
    }
    pub fn sender(&self) -> &ComponentSender {
        &self.sender
    }
}

// A utility function that can be use when sending
#[inline]
pub fn send_cmd<T>(sender: &Sender<T>, cmd: T)
where
    T: MachineImpl + MachineImpl<InstructionSet = T> + std::fmt::Debug,
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
    use simplelog::*;

    use crate::settings::Service;
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
                println!("recv");
                match cmd {
                    ComponentCmd::NewSession(conn_id, service, sender) => {
                        assert_eq!(conn_id, 12345);
                        assert_eq!(service, Service::EchoServer);
                        assert_eq!(
                            false,
                            Arc::clone(&sender)
                                .downcast::<Sender<TestMessage>>()
                                .is_ok()
                        );
                        assert_eq!(
                            true,
                            Arc::clone(&sender)
                                .downcast::<Sender<ComponentCmd>>()
                                .is_ok()
                        );
                        self.counter.store(1);
                    }
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
        CombinedLogger::init(vec![TermLogger::new(
            LevelFilter::Error,
            Config::default(),
            TerminalMode::Mixed,
        )])
        .unwrap();
        // tweaks for more responsive testing
        executor::set_selector_maintenance_duration(std::time::Duration::from_millis(20));

        executor::start_server();
        thread::sleep(std::time::Duration::from_millis(20));
        let (m, component_sender) = executor::connect::<_, ComponentCmd>(Controller::default());
        let _test_sender = executor::and_connect::<_, TestMessage>(&m);

        if component_sender.send(ComponentCmd::NewSession(
            12345,
            Service::EchoServer,
            Arc::new(component_sender.clone()),
        )).is_err(){}
        thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(m.lock().unwrap().counter.load(), 1);
        executor::stop_server();
    }
}
