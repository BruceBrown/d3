#![allow(dead_code)]
use super::*;


/// This is the entry point for configuring the producer component. We get two sets of settings,
/// our specific and the full set. Hopefully we'll not need to dig into the full set
/// In our case, there's very little to do.
pub fn configure(config: SimpleConfig, _settings: &settings::Settings) -> Result<Option<ComponentSender>, Box<dyn Error>> {
    if config.enabled {
        let (_, sender) = executor::connect(ProducerComponent::new());
        Ok(Some(sender))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct ProducerComponent {
}
impl ProducerComponent {
    pub fn new() -> Self { Self {} }

    // create a producer for the new connection
    fn create_instance(&self, conn_id: u128, any_sender: AnySender) {
        if let Ok(session_sender) = Arc::clone(&any_sender).downcast::<Sender<ChatCmd>>() {
            let (_instance, sender) = executor::connect(ChatInstance::new(conn_id));
            send_cmd(&session_sender, ChatCmd::Instance(conn_id, sender, settings::Component::ChatProducer));
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
            ComponentCmd::NewSession(conn_id, service, any_sender) if service == Service::ChatServer => self.create_instance(conn_id, any_sender),
            ComponentCmd::Start => (),
            ComponentCmd::Stop => (),
            _=> (),
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
    fn new(session_id: u128) -> Self { Self {session_id, ..Default::default() } }

    // add a sink, as it turns out the only sink we add is to the coordinator
    fn add_sink(&self, conn_id: u128, sender: ChatSender) {
        log::debug!("producer {} adding sink {}", self.session_id, conn_id);
        self.mutable.lock().unwrap().senders.push(sender);
    }

    // remove a sink. This justs gets forwarded to the coordinator
    fn remove_sink(&self, conn_id: u128) {
        // this should be our connection going away...
        if conn_id != self.session_id {
            log::warn!("producer {} asked to remove {}", self.session_id, conn_id);
            return
        }
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("producer {} sending RemoveSink to coordinator", self.session_id);
            send_cmd(&s, ChatCmd::RemoveSink(conn_id));
        });
    }

    // new data is forwarded
    fn new_data(&self, _conn_id: u128, bytes: &Data) {
        self.mutable.lock().unwrap().senders.iter().for_each(|s| {
            log::debug!("producer {} sending to coordinator", self.session_id);
            send_cmd(&s, ChatCmd::NewData(self.session_id, Arc::clone(bytes)));
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
            ChatCmd::AddSink(conn_id, sender) => self.add_sink(conn_id, sender),
            ChatCmd::RemoveSink(conn_id) => self.remove_sink(conn_id),
            ChatCmd::NewData(conn_id, ref bytes) => self.new_data(conn_id, bytes),
            _ => log::warn!("echo reader received an echo cmd it doesn't handle: {:#?}", cmd),
        }
    }
}