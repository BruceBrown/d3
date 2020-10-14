//! The chat server consumer.
//! Configuration is performed via the public configure function. This is the only public exposure.
//!
//! The theory of operation is that this component cooperates with other components to form a chat server.
//! This is the consumer component, which is wired between the coordinator and producer. Although, it isn't
//! actually aware of that wiring. It just receives instructions and acts accordingly.
//!
//! At startup, configure() is called, providing configuration for the component, which can return 1 of 3 ways:
//! * If the component is not enabled, Ok(None) is returned.
//! * If there is an error in the config, and the component can't be created, Err(ComponentError) is returned.
//! * Otherwise, the component is created and connected with its sender being returned Ok(ComponentSender).
//!
//! When a connection occurs, the component will receive a NewSession command. That command includes a connection
//! id, service id, and a sender. If the service is not the ChatService, the command is ignored. Otherwise,
//! the sender is decoded into a ChatSender, a consumer instance is created and the instance's sender, along
//! with the component type of ChatConsumer is sent to the ChatSender.
//!
//! The ChatInstance, receives ChatCmd commands. It is told to:
//! * Add a sender to the list of senders.
//! * Remove a sender from the list of senders.
//! * NewData which is send to the list of senders.
//!
//! As you can now imagine, the consumer instance performs a fanout of received data.
//!
//! While this could have been written simpler, using a single component, it is an objective to illustrate
//! how components interact and are wired together.
//!
use super::*;

use std::collections::HashMap;

// The entry point for configuring the chat consumer component. It receives two sets of settings,
// component specific and the full set. In most cases the specific set is sufficient.
// In this case, there's very little to do.
pub fn configure(
    config: SimpleConfig,
    _settings: &settings::Settings,
) -> Result<Option<ComponentSender>, ComponentError> {
    if config.enabled {
        let (_, sender) = executor::connect(ConsumerComponent::new());
        Ok(Some(sender))
    } else {
        Ok(None)
    }
}

/// The ConsumerComponent is a factory for a ChatInstance, a consumer of chat messages.
#[derive(Debug, Default)]
struct ConsumerComponent {}

impl ConsumerComponent {
    pub fn new() -> Self { Self {} }

    // create a consumer for the new session
    fn create_instance(&self, conn_uuid: u128, any_sender: AnySender) {
        if let Ok(session_sender) = Arc::clone(&any_sender).downcast::<ChatSender>() {
            let (_instance, sender) = executor::connect(ChatInstance::new(conn_uuid));
            send_cmd(
                &session_sender,
                ChatCmd::Instance(conn_uuid, sender, settings::Component::ChatConsumer),
            );
        } else {
            log::warn!("chat consumer component received an unknown sender")
        }
    }
}

impl Machine<ComponentCmd> for ConsumerComponent {
    fn disconnected(&self) {
        log::info!("chat consumer component disconnected");
    }

    fn receive(&self, cmd: ComponentCmd) {
        match cmd {
            ComponentCmd::NewSession(conn_uuid, service, any_sender) if service == Service::ChatServer => {
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
    sender_producer: Option<ChatSender>,
    senders: HashMap<u128, ChatSender>,
}

#[derive(Debug, Default)]
struct ChatInstance {
    session_id: u128,
    mutable: Mutex<ChatInstanceData>,
}
impl ChatInstance {
    // create a new consumer instance
    fn new(session_id: u128) -> Self {
        Self {
            session_id,
            ..Default::default()
        }
    }

    // add a sink, which is a producer's sender
    fn add_sink(&self, conn_uuid: u128, sender: ChatSender) {
        let mut mutable = self.mutable.lock().unwrap();
        if conn_uuid == self.session_id {
            // if this is our producer save it for later (shutdown)
            mutable.sender_producer.replace(sender);
            log::debug!("consumer {} producer is set", self.session_id);
        } else {
            // otherwise, this is a producer we'll forward to
            log::debug!("consumer {} sending input to producer {}", self.session_id, conn_uuid);
            mutable.senders.insert(conn_uuid, sender);
        }
    }

    // remove a sink, which is a producer's sender
    fn remove_sink(&self, conn_uuid: u128) {
        let mut mutable = self.mutable.lock().unwrap();
        // it its our connection, let the producer know
        if conn_uuid == self.session_id {
            if let Some(sender) = &mutable.sender_producer {
                // tell our producer, who we've otherwise never spoken with...
                send_cmd(sender, ChatCmd::RemoveSink(conn_uuid));
            }
        } else {
            // otherwise, stop forwarding to it
            log::debug!("consumer {} removed producer {}", self.session_id, conn_uuid);
            mutable.senders.remove(&conn_uuid);
        }
    }

    // a new message arrived, forward it to saved producers
    fn new_data(&self, conn_uuid: u128, bytes: &Data) {
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("consumer {} sending to {}", self.session_id, s.0);
            send_cmd(&s.1, ChatCmd::NewData(conn_uuid, Arc::clone(bytes)));
        });
    }
}

impl Machine<ChatCmd> for ChatInstance {
    fn disconnected(&self) {
        log::info!("chat consumer {} disconnected", self.session_id);
        // cleanup
        let mut mutable = self.mutable.lock().unwrap();
        mutable.senders.clear();
        mutable.sender_producer.take();
    }

    fn receive(&self, cmd: ChatCmd) {
        match cmd {
            ChatCmd::AddSink(conn_uuid, sender) => self.add_sink(conn_uuid, sender),
            ChatCmd::RemoveSink(conn_uuid) => self.remove_sink(conn_uuid),
            ChatCmd::NewData(conn_uuid, ref bytes) => self.new_data(conn_uuid, bytes),
            _ => log::warn!("chat consumer received a cmd it doesn't handle: {:#?}", cmd),
        }
    }
}
