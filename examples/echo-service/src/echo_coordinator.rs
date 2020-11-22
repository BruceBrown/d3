use super::*;
use components::{ComponentError, ComponentInfo, ComponentSender};
use network::{get_network_sender, NetCmd, NetConnId, NetSender};
use settings::{CoordinatorVariant, Settings};
use std::net::SocketAddr;

// maybe a trait would be better
#[allow(dead_code)]
pub fn configure(settings: &Settings, components: &[ComponentInfo]) -> Result<Option<ComponentSender>, ComponentError> {
    // ensure our service is configure in services
    if !settings.services.contains("EchoService") {
        log::debug!("echo service is not configured");
        return Ok(None);
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(value) = c.get("EchoCoordinator") {
            let maybe_coordinator = match value {
                CoordinatorVariant::SimpleTcpConfig { tcp_address, .. } => Some(EchoCoordinator {
                    net_sender: get_network_sender(),
                    bind_addr: tcp_address.to_string(),
                    my_sender: Mutex::new(None),
                    components: components.to_owned(),
                }),
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };
            if let Some(coordinator) = maybe_coordinator {
                if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                    log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr);
                    continue;
                }
                log::debug!("echo service selected configuration: {:#?}", value);
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                // save out network sender until we give it away during start
                m.my_sender.lock().replace(sender);
                let sender = executor::and_connect::<_, ComponentCmd>(&m);
                return Ok(Some(sender));
            }
        }
    }
    log::warn!("The configuration for the echo service coordinator was not found.");
    Err(ComponentError::BadConfig(
        "The configuration for the echo service coordinator was not found.".to_string(),
    ))
}

#[derive(Debug)]
struct EchoCoordinator {
    net_sender: NetSender,
    my_sender: Mutex<Option<NetSender>>,
    bind_addr: String,
    components: Vec<ComponentInfo>,
}

impl EchoCoordinator {
    fn create_instance(&self, conn_id: NetConnId, buf_size: usize) {
        let conn_uuid: u128 = conn_id.try_into().unwrap();
        // create an instance to handle the connection, the EchoInstance has two instruction sets.
        // Wire them both to the same instance.
        let (instance, sender) = executor::connect::<_, EchoCmd>(EchoInstance::new(conn_uuid, buf_size, self.net_sender.clone()));
        instance.my_sender.lock().replace(sender.clone());
        // the the other components that there's a new echo session nad how to contact the coordinator
        self.components.iter().for_each(|c| {
            send_cmd(
                c.sender(),
                ComponentCmd::NewSession(conn_uuid, "EchoService".to_string(), Arc::new(sender.clone())),
            )
        });
        // tell the network where to send connection control and data
        let sender = executor::and_connect::<_, NetCmd>(&instance);
        send_cmd(&self.net_sender, NetCmd::BindConn(conn_id, sender));
    }
}

impl Machine<ComponentCmd> for EchoCoordinator {
    fn receive(&self, cmd: ComponentCmd) {
        match cmd {
            ComponentCmd::Start => {
                let my_sender = self.my_sender.lock().take();
                send_cmd(
                    &self.net_sender,
                    NetCmd::BindListener(self.bind_addr.to_string(), my_sender.unwrap()),
                );
            },
            ComponentCmd::Stop => (),
            _ => (),
        }
    }
}

impl Machine<NetCmd> for EchoCoordinator {
    fn disconnected(&self) {
        log::warn!("echo router component disconnected");
    }
    fn receive(&self, cmd: NetCmd) {
        // log::trace!("echo instance {:#?}", &cmd);
        // for a single match, this is cleaner
        if let NetCmd::NewConn(conn_id, _bind_addr, _remote_addr, buf_size) = cmd {
            self.create_instance(conn_id, buf_size);
        }
    }
}

// We have the echo instance, which interfaces with the network.
// We're going to hear back from 2 instances, one is consumer souce/sink
// the other is a producer source/sink. The echo instance is going to wire
// things up such that network traffic flows into the echo instance,
// from there if flows into the consumer. The consumer flows it into
// the producer and finally the producer flows it into the controller
// where it is written out. Why so complex? well, later we'll change
// one thing to turn it into a chat service.

#[derive(Debug)]
struct EchoInstance {
    conn_id: u128,
    buf_size: usize,
    net_sender: NetSender,
    // its dangerous to hold onto this sender, it represents a circularity
    my_sender: Mutex<Option<EchoCmdSender>>,
    consumer: Mutex<Option<EchoCmdSender>>,
    producer: Mutex<Option<EchoCmdSender>>,
}
impl EchoInstance {
    fn new(conn_id: u128, buf_size: usize, net_sender: NetSender) -> Self {
        Self {
            conn_id,
            buf_size,
            net_sender,
            my_sender: Mutex::default(),
            consumer: Mutex::default(),
            producer: Mutex::default(),
        }
    }

    fn close(&self, _conn_id: NetConnId) {
        // drop all, producer and my_sender should alerady be dropped
        self.my_sender.lock().take();
        self.consumer.lock().take();
        self.producer.lock().take();
    }

    fn new_data(&self, conn_uuid: u128, bytes: EchoData) {
        let conn_id: usize = conn_uuid.try_into().unwrap();
        let vec_bytes: Vec<u8> = match Arc::try_unwrap(bytes) {
            Ok(v) => v,
            Err(v) => v.to_vec(),
        };
        send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, vec_bytes));
    }

    fn add_consumer(&self, sender: EchoCmdSender) {
        let ready = self.producer.lock().is_some();
        if ready {
            log::info!("getting producer and wiring");
            let producer_sender = self.producer.lock().take().unwrap();
            self.wire_instances(sender, producer_sender);
        } else {
            log::info!("saving consumer");
            self.consumer.lock().replace(sender);
        }
    }

    fn add_producer(&self, sender: EchoCmdSender) {
        let ready = self.consumer.lock().is_some();
        if ready {
            log::info!("getting consumer and wiring");
            let consumer_sender = self.consumer.lock().take().unwrap();
            self.wire_instances(consumer_sender, sender);
        } else {
            log::info!("saving producer");
            self.producer.lock().replace(sender);
        }
    }

    // wire as follows:  self -> consumer -> producer -> self
    fn wire_instances(&self, consumer_sender: EchoCmdSender, producer_sender: EchoCmdSender) {
        // we send to the consumer, which sends to the producer, which sends back to us.
        let my_sender = self.my_sender.lock().take().unwrap();
        send_cmd(&producer_sender, EchoCmd::AddSink(self.conn_id, my_sender));
        send_cmd(&consumer_sender, EchoCmd::AddSink(self.conn_id, producer_sender));
        self.consumer.lock().replace(consumer_sender);
        log::info!("all wired, saved consumer");
    }

    // received bytes from the network
    fn received_bytes(&self, conn_id: NetConnId, bytes: Vec<u8>) {
        let conn_uuid: u128 = conn_id.try_into().unwrap();
        let bytes = Arc::new(bytes);
        send_cmd(self.consumer.lock().as_ref().unwrap(), EchoCmd::NewData(conn_uuid, bytes));
    }
}

impl Machine<EchoCmd> for EchoInstance {
    fn disconnected(&self) {
        log::info!("echo router (EchoCmd) disconnected");
    }
    fn receive(&self, cmd: EchoCmd) {
        // log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            EchoCmd::Instance(_conn_id, sender, component) if component == "EchoConsumer" => self.add_consumer(sender),
            EchoCmd::Instance(_conn_id, sender, component) if component == "EchoProducer" => self.add_producer(sender),
            EchoCmd::NewData(conn_id, bytes) => self.new_data(conn_id, bytes),
            _ => (),
        }
    }
}

impl Machine<NetCmd> for EchoInstance {
    fn disconnected(&self) {
        log::info!("echo router (NetCmd) disconnected");
    }
    fn receive(&self, cmd: NetCmd) {
        // log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            NetCmd::CloseConn(conn_id) => self.close(conn_id),
            NetCmd::RecvBytes(conn_id, bytes) => self.received_bytes(conn_id, bytes),
            NetCmd::SendReady(_conn_id, _max_bytes) => (),
            _ => log::warn!("echo router received unexpected network command"),
        }
    }
}
