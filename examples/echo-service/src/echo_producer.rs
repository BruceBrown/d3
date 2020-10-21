#![allow(dead_code)]
use super::*;

/// This is the entry point for configuring the producer component. We get two sets of settings,
/// our specific and the full set. Hopefully we'll not need to dig into the full set
/// In our case, there's very little to do.
pub fn configure(
    config: SimpleConfig,
    _settings: &settings::Settings,
) -> Result<Option<ComponentSender>, ComponentError> {
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
    fn create_instance(&self, conn_id: u128, any_sender: AnySender) {
        if let Ok(session_sender) = Arc::clone(&any_sender).downcast::<Sender<EchoCmd>>() {
            let (_instance, sender) = executor::connect(EchoInstance::new(conn_id));
            send_cmd(
                &session_sender,
                EchoCmd::Instance(conn_id, sender, "EchoProducer".to_string()),
            );
        } else {
            log::warn!("echo producer component received an unknown sender")
        }
    }
}

impl Machine<ComponentCmd> for ProducerComponent {
    fn disconnected(&self) {
        log::info!("echo producer component disconnected");
    }

    fn receive(&self, cmd: ComponentCmd) {
        match cmd {
            ComponentCmd::NewSession(conn_id, service, any_sender) if service == "EchoServer" => {
                self.create_instance(conn_id, any_sender)
            },
            ComponentCmd::Start => (),
            ComponentCmd::Stop => (),
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
struct EchoInstanceData {
    senders: Vec<EchoCmdSender>,
}

#[derive(Debug, Default)]
struct EchoInstance {
    session_id: u128,
    mutable: Mutex<EchoInstanceData>,
}
impl EchoInstance {
    fn new(session_id: u128) -> Self {
        Self {
            session_id,
            ..Default::default()
        }
    }
    fn add_sink(&self, sender: EchoCmdSender) { self.mutable.lock().unwrap().senders.push(sender); }
    fn new_data(&self, conn_id: u128, bytes: &EchoData) {
        self.mutable
            .lock()
            .unwrap()
            .senders
            .iter()
            .for_each(|s| send_cmd(s, EchoCmd::NewData(conn_id, Arc::clone(bytes))));
    }
}

impl Machine<EchoCmd> for EchoInstance {
    fn disconnected(&self) {
        log::info!("echo producer disconnected");
    }

    fn receive(&self, cmd: EchoCmd) {
        match cmd {
            EchoCmd::AddSink(_, sender) => self.add_sink(sender),
            EchoCmd::NewData(conn_id, ref bytes) => self.new_data(conn_id, bytes),
            _ => log::warn!("echo producer received an echo cmd it doesn't handle: {:#?}", cmd),
        }
    }
}
