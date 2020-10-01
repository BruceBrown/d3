#![allow(dead_code)]
use super::*;

use std::collections::HashMap;

/// This is the entry point for configuring the consumer component. We get two sets of settings,
/// our specific and the full set. Hopefully we'll not need to dig into the full set
/// In our case, there's very little to do.
pub fn configure(config: SimpleConfig, _settings: &settings::Settings) -> Result<Option<ComponentSender>, Box<dyn Error>> {
    if config.enabled {
        let (_, sender) = executor::connect(ConsumerComponent::new());
        Ok(Some(sender))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct ConsumerComponent {
}

impl ConsumerComponent {
    pub fn new() -> Self { Self {} }
    
    // create a consumer for the new connection
    fn create_instance(&self, conn_id: u128, any_sender: AnySender) {
        if let Ok(session_sender) = Arc::clone(&any_sender).downcast::<Sender<ChatCmd>>() {
            let (_instance, sender) = executor::connect(ChatInstance::new(conn_id));
            send_cmd(&session_sender, ChatCmd::Instance(conn_id, sender, settings::Component::ChatConsumer));
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
            ComponentCmd::NewSession(conn_id, service, any_sender) if service == Service::ChatServer => self.create_instance(conn_id, any_sender),
            ComponentCmd::Start => (),
            ComponentCmd::Stop => (),
            _=> (),
        }
    }
}

#[derive(Debug, Default)]
struct ChatInstanceData {
    sender_producer: Option<ChatSender>,
    senders: HashMap<u128,ChatSender>,
}

#[derive(Debug, Default)]
struct ChatInstance {
    session_id: u128,
    mutable: Mutex<ChatInstanceData>,
}
impl ChatInstance {
    // create a new consumer instance
    fn new(session_id: u128) -> Self { Self {session_id, ..Default::default() } }

    // add a sink, which is a producer's sender
    fn add_sink(&self, conn_id: u128, sender: ChatSender) {
        let mut mutable = self.mutable.lock().unwrap();
        if conn_id == self.session_id {
            // if this is our producer save it for later (shutdown)
            mutable.sender_producer.replace(sender);
            log::debug!("consumer {} producer is set", self.session_id);
        } else {
            // otherwise, this is a producer we'll forward to
            log::debug!("consumer {} sending input to producer {}", self.session_id, conn_id);
            mutable.senders.insert(conn_id, sender);
        }
    }

    // remove a sink, which is a producer's sender 
    fn remove_sink(&self, conn_id: u128) {
        let mut mutable = self.mutable.lock().unwrap();
        // it its our connection, let the producer know
        if conn_id == self.session_id {
            if let Some(sender) = &mutable.sender_producer {
                // tell our producer, who we've otherwise never spoken with...
                send_cmd(sender, ChatCmd::RemoveSink(conn_id));
            }
        } else {
            // otherwise, stop forwarding to it
            log::debug!("consumer {} removed producer {}", self.session_id, conn_id);
            mutable.senders.remove(&conn_id);
        }
    }

    // a new message arrived, forward it to saved producers
    fn new_data(&self, conn_id: u128, bytes: &Data) {
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("consumer {} sending to {}", self.session_id, s.0);
            send_cmd(&s.1, ChatCmd::NewData(conn_id, Arc::clone(bytes)));
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
            ChatCmd::AddSink(conn_id, sender) => self.add_sink(conn_id, sender),
            ChatCmd::RemoveSink(conn_id) => self.remove_sink(conn_id),
            ChatCmd::NewData(conn_id, ref bytes) => self.new_data(conn_id, bytes),
            _ => log::warn!("chat consumer received a cmd it doesn't handle: {:#?}", cmd),
        }
    }
}