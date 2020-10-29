//! The chat server producer.
//! Configuration is performed via the public configure function. This is the only public exposure.
//!
//! The theory of operation is that this component cooperates with other components to form a chat server.
//! This is the producer component, which is wired between the consumer and coordinator. Although, it isn't
//! actually aware of that wiring. It just receives instructions and acts accordingly.
//!
//! At startup, configure() is called, providing configuration for the component, which can return 1 of 3 ways:
//! * If the component is not enabled, Ok(None) is returned.
//! * If there is an error in the config, and the component can't be created, Err(ComponentError) is returned.
//! * Otherwise, the component is created and connected with its sender being returned Ok(ComponentSender).
//!
//! When a connection occurs, the component will receive a NewSession command. That command includes a connection
//! id, service id, and a sender. If the service is not the ChatService, the command is ignored. Otherwise,
//! the sender is decoded into a ChatSender, a producer instance is created and the instance's sender, along
//! with the component type of ChatProducer is sent to the ChatSender.
//!
//! The ChatInstance, receives ChatCmd commands. It is told to:
//! * Add a sender to the list of senders.
//! * Remove a sender from the list of senders.
//! * NewData which is send to the list of senders.
//!
//! As you can now imagine, the producer instance performs a fanout of received data. In many ways there
//! is little distintion between the consumer and producer. It all comes down to wiring.
//!
//! While this could have been written simpler, using a single component, it is an objective to illustrate
//! how components interact and are wired together.
#![allow(dead_code)]
use super::*;

// The entry point for configuring the chat producer component. It receives two sets of settings,
// component specific and the full set. In most cases the specific set is sufficient.
// In this case, there's very little to do.
pub fn configure(config: SimpleConfig, _settings: &settings::Settings) -> Result<Option<ComponentSender>, ComponentError> {
    if config.enabled {
        let (_, sender) = executor::connect(ProducerComponent::new());
        Ok(Some(sender))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct ProducerComponent {}
impl ProducerComponent {
    pub const fn new() -> Self { Self {} }

    // create a producer for the new connection
    fn create_instance(&self, conn_uuid: u128, any_sender: AnySender) {
        if let Ok(session_sender) = Arc::clone(&any_sender).downcast::<Sender<ChatCmd>>() {
            let (_instance, sender) = executor::connect(ChatInstance::new(conn_uuid));
            send_cmd(&session_sender, ChatCmd::Instance(conn_uuid, sender, "ChatProducer".to_string()));
        } else {
            log::warn!("echo producer component received an unknown sender");
        }
    }
}

impl Machine<ComponentCmd> for ProducerComponent {
    fn disconnected(&self) {
        log::info!("chat producer component disconnected");
    }

    fn receive(&self, cmd: ComponentCmd) {
        match cmd {
            ComponentCmd::NewSession(conn_uuid, service, any_sender) if service == "ChatServer" => {
                self.create_instance(conn_uuid, any_sender)
            },
            ComponentCmd::Start => (),
            ComponentCmd::Stop => (),
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
struct ChatInstanceData {
    senders: Vec<ChatSender>,
}

#[derive(Debug, Default)]
struct ChatInstance {
    session_id: u128,
    mutable: Mutex<ChatInstanceData>,
}
impl ChatInstance {
    fn new(session_id: u128) -> Self {
        Self {
            session_id,
            ..Default::default()
        }
    }

    // add a sink, as it turns out the only sink we add is to the coordinator
    fn add_sink(&self, conn_uuid: u128, sender: ChatSender) {
        log::debug!("producer {} adding sink {}", self.session_id, conn_uuid);
        self.mutable.lock().unwrap().senders.push(sender);
    }

    // remove a sink. This justs gets forwarded to the coordinator
    fn remove_sink(&self, conn_uuid: u128) {
        // this should be our connection going away...
        if conn_uuid != self.session_id {
            log::warn!("producer {} asked to remove {}", self.session_id, conn_uuid);
            return;
        }
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("producer {} sending RemoveSink to coordinator", self.session_id);
            send_cmd(s, ChatCmd::RemoveSink(conn_uuid));
        });
    }

    // new data is forwarded
    fn new_data(&self, _conn_id: u128, bytes: &Data) {
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("producer {} sending to coordinator", self.session_id);
            send_cmd(s, ChatCmd::NewData(self.session_id, Arc::clone(bytes)));
        });
    }
}

impl Machine<ChatCmd> for ChatInstance {
    fn disconnected(&self) {
        log::info!("chat producer {} disconnected", self.session_id);
        // cleanup
        let mut mutable = self.mutable.lock().unwrap();
        mutable.senders.clear();
    }

    fn receive(&self, cmd: ChatCmd) {
        match cmd {
            ChatCmd::AddSink(conn_uuid, sender) => self.add_sink(conn_uuid, sender),
            ChatCmd::RemoveSink(conn_uuid) => self.remove_sink(conn_uuid),
            ChatCmd::NewData(conn_uuid, ref bytes) => self.new_data(conn_uuid, bytes),
            _ => log::warn!("chat producer received a cmd it doesn't handle: {:#?}", cmd),
        }
    }
}
