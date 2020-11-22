//! The chat coordinator.
//! Configuration is performed via the public configure function. This is the only public exposure.
//!
//! The theory of operation is that this component cooperates with other components to form a chat service.
//! This is the coordinator component, which is wired before the consumer and after the producer. It is
//! responsible for wiring instances together and interacting with the network.
//!
//! At startup, configure() is called, providing configuration for the component, which can return 1 of 3 ways:
//! * If the component is not enabled, Ok(None) is returned.
//! * If there is an error in the config, and the component can't be created, Err(ComponentError) is returned.
//! * Otherwise, the component is created and connected with its sender being returned Ok(ComponentSender).
//! Unlike the other chat components, the coordinator interacts with the network. It will validate the
//! address that it is told to bind to.
//!
//! When a `Component::Start` command is received it will send a `NetCmd::BindListener` to the Network via
//! the network sender.
//! When a `Component::Stop` command is received, it will drop its senders, thereby collapsing the network
//! of instances and components.
//!
//! When the `ChatCoordinator` receives a NetCmd::NewConn it is being told that a connection has been accepted. In
//! response, it tells the network where to send control and data, via `NetCmd::BindConn`. For the chat service,
//! the Chat has a single `ChatInstance` that is responsible for all connections. In addition to telling the
//! binding the connection, the `ChatCoordinator` sends a ComponentCmd::NewSession to every component. That
//! instruction includes the ChatService and the `ChatInstance's sender`.
//!
//! The `ChatInstance` can receive several different NetCmd, which it needs to handle.
//! * CloseConn, informs it that the connection is closed. All connections are sent a RemoveSink for the connection.
//! * RecvBytes, provides new bytes of data, which are sent to the consumer as ChatCmd::NewData
//! * SendReady, provides the number of bytes the network is ready to send
//!
//! The `ChatInstance` can receive several different ChatCmd, which it needs to handle.
//! * Instance, informs it of a new component instance and its roll. For a consumer, it is told to
//! add sender for all of the producers which are maintained in a map owned by the `ChatInstance`. For a
//! producer instance, all consumers are told to add the producer to its list of senders. The producer
//! is told to add the `ChatInstance's sender` to its list of senders. The end result is that the 'ChatInstance'
//! has a map of all producers and senders and has wired all of the consumers to all of the producers and wired
//! all of the producers to itself. This allows for a message to come in on a connection, get routed to the
//! consumer for that message, which then sends it to all of the producers, which in turn send it to the `ChatInstance`
//! which sends it to all of the network connections.
//! * RemoveSink, informs the `ChatInstance` that it can safely drop the consumer and producer for the closed session.
//! * NewData, informs the `ChatInstance` that there is data to be sent to the network connection.
//!
//! While this could have been written simpler, using a single component, it is an objective to illustrate
//! how components interact and are wired together.
use super::*;

use std::collections::HashMap;
use std::net::SocketAddr;

/// Not quite as simple as the echo service, which has a pathway of Coordinate -> Consumer -> Producer -> Coordinator
///
/// Normally, we'd add a new component to manage routing with multiple chat rooms. However, we're going to cheap
/// out and just have a single room.
pub fn configure(settings: &Settings, components: &[ComponentInfo]) -> Result<Option<ComponentSender>, ComponentError> {
    // ensure our service is configure in services
    if !settings.services.contains("ChatService") {
        log::debug!("chat service is not configured");
        return Ok(None);
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(v) = c.get("ChatCoordinator") {
            let coordinator = match v {
                CoordinatorVariant::SimpleTcpConfig { tcp_address, kv } if kv.is_some() => {
                    let mutable = Mutex::new(MutableCoordinatorData {
                        kv: kv.as_ref().unwrap().clone(),
                        ..MutableCoordinatorData::default()
                    });
                    Some(ChatCoordinator {
                        net_sender: get_network_sender(),
                        bind_addr: tcp_address.to_string(),
                        components: components.to_owned(),
                        mutable,
                    })
                },
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };
            if let Some(coordinator) = coordinator {
                {
                    // maybe move up and into the match
                    let mutable = coordinator.mutable.lock();
                    if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                        log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr);
                        continue;
                    }
                    if !mutable.kv.contains_key(&"name_prompt".to_string()) {
                        log::error!("chat service is missing keyword name_prompt from config");
                        continue;
                    }
                }
                log::debug!("chat service selected configuration: {:#?}", v);
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                // save out network sender until we give it away during start
                m.mutable.lock().my_sender.replace(sender);
                let sender = executor::and_connect::<_, ComponentCmd>(&m);
                return Ok(Some(sender));
            }
        }
    }
    log::warn!("The configuration for the chat service coordinator was not found.");
    Err(ComponentError::BadConfig(
        "The configuration for the chat service coordinator was not found.".to_string(),
    ))
}

#[derive(Debug, Default)]
struct MutableCoordinatorData {
    my_sender: Option<NetSender>,
    kv: HashMap<String, String>,
    /// Instance's Net sender
    inst_net_sender: Option<NetSender>,
    /// Instance's ChatCmd sender
    inst_chat_sender: Option<ChatSender>,
}
impl MutableCoordinatorData {
    // create a coordinator it it hasn't already been created. Then ask the compoents to create
    // instances, which the coordinator will assemble.
    fn create_instance(&mut self, conn_id: NetConnId, buf_size: usize, net_sender: &NetSender, components: &[ComponentInfo]) {
        // create an instance to handle the connection, the ChatInstance has two instruction sets.
        // Wire them both to the same instance.
        // Normally, you'd build a translation to a conn_uuid.
        let conn_uuid: u128 = conn_id.try_into().unwrap();
        if self.inst_chat_sender.is_none() {
            // get the instance running
            let (instance, sender) = executor::connect::<_, ChatCmd>(ChatInstance::new(
                conn_uuid,
                std::mem::take(&mut self.kv),
                buf_size,
                net_sender.clone(),
            ));
            // save the chat sender in the coordinator
            self.inst_chat_sender.replace(sender.clone());
            // save the chat sender in the instance
            instance.my_sender.lock().replace(sender);
            // create a net sender for the instance
            let sender = executor::and_connect::<_, NetCmd>(&instance);
            // save the sender in the instance
            self.inst_net_sender.replace(sender);
        }
        // tell the network that the coordinator will handle the connection traffic
        let sender = self.inst_net_sender.as_ref().unwrap().clone();
        send_cmd(net_sender, NetCmd::BindConn(conn_id, sender));
        // let all of the components know that there's a new session for the chat service and how to contact the coordinator
        let sender = self.inst_chat_sender.as_ref().unwrap().clone();
        components.iter().for_each(|c| {
            send_cmd(
                c.sender(),
                ComponentCmd::NewSession(conn_uuid, "ChatService".to_string(), Arc::new(sender.clone())),
            )
        });
    }
}

#[derive(Debug)]
struct ChatCoordinator {
    net_sender: NetSender,
    bind_addr: String,
    components: Vec<ComponentInfo>,
    mutable: Mutex<MutableCoordinatorData>,
}

impl ChatCoordinator {
    fn create_instance(&self, conn_id: NetConnId, buf_size: usize) {
        let mut mutable = self.mutable.lock();
        mutable.create_instance(conn_id, buf_size, &self.net_sender, &self.components);
    }
}

impl Machine<ComponentCmd> for ChatCoordinator {
    fn receive(&self, cmd: ComponentCmd) {
        let mut mutable = self.mutable.lock();
        match cmd {
            ComponentCmd::Start => {
                let my_sender = mutable.my_sender.take();
                send_cmd(
                    &self.net_sender,
                    NetCmd::BindListener(self.bind_addr.to_string(), my_sender.unwrap()),
                );
            },
            ComponentCmd::Stop => {
                mutable.inst_net_sender.take();
                mutable.inst_chat_sender.take();
                mutable.my_sender.take();
            },
            _ => (),
        }
    }
}

impl Machine<NetCmd> for ChatCoordinator {
    fn disconnected(&self) {
        log::warn!("chat coordinator component disconnected");
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

#[derive(Debug, Default)]
struct MutableData {
    consumers: HashMap<u128, ChatSender>,
    producers: HashMap<u128, ChatSender>,
    names: HashMap<u128, Vec<u8>>,
}

#[derive(Debug)]
struct ChatInstance {
    conn_id: u128,
    kv: HashMap<String, String>,
    buf_size: usize,
    net_sender: NetSender,
    // its dangerous to hold onto this sender, it represents a circularity
    my_sender: Mutex<Option<ChatSender>>,
    mutable: Mutex<MutableData>,
}
impl ChatInstance {
    fn new(conn_id: u128, kv: HashMap<String, String>, buf_size: usize, net_sender: NetSender) -> Self {
        Self {
            conn_id,
            kv,
            buf_size,
            net_sender,
            my_sender: Mutex::default(),
            mutable: Mutex::new(MutableData::default()),
        }
    }

    // notification that a connection has closed. let everyone know that its gone
    fn close(&self, conn_id: usize) {
        // we only do a fast drop for connections that never fully connected
        let conn_uuid: u128 = conn_id.try_into().unwrap();
        let mut fast_drop = true;
        log::debug!("coordinator notifed that connection {} is gone", conn_id);
        if let Some(gone) = self.kv.get(&"gone_message".to_string()) {
            let mutable = self.mutable.lock();
            if let Some(name) = mutable.names.get(&conn_uuid) {
                if let Some(sender) = mutable.consumers.get(&conn_uuid) {
                    fast_drop = false;
                    let mut msg = name.clone();
                    msg.extend(gone.as_bytes().to_vec());
                    // send out the leave notification
                    send_cmd(sender, ChatCmd::NewData(conn_uuid, Arc::new(msg)));
                }
            }
            if !fast_drop {
                // let everyone know that the connection is gone
                for sender in mutable.consumers.values() {
                    send_cmd(sender, ChatCmd::RemoveSink(conn_uuid));
                }
            }
        }
        if fast_drop {
            self.remove_sink(conn_uuid);
        }
    }

    // final removal of the connection, this will cause the consumer and producer to die as well
    fn remove_sink(&self, conn_uuid: u128) {
        // we've come full circle from where we told the consumers that the connection is closed.
        log::debug!("coordinator is dropping name, consumer and producer for connection {}", conn_uuid);
        let mut mutable = self.mutable.lock();
        mutable.names.remove(&conn_uuid);
        mutable.producers.remove(&conn_uuid);
        mutable.consumers.remove(&conn_uuid);
    }

    // new data has arrived, we don't send to the network unless fully connected
    fn new_data(&self, conn_uuid: u128, bytes: Data) {
        // normally we'd translate, but here we'll jst convert
        let conn_id: usize = conn_uuid.try_into().unwrap();
        let vec_bytes: Vec<u8> = match Arc::try_unwrap(bytes) {
            Ok(v) => v,
            Err(v) => v.to_vec(),
        };
        let mutable = self.mutable.lock();
        // no peeking without first giving a name
        if mutable.names.contains_key(&conn_uuid) {
            log::debug!("coordinator sending to {}", conn_id);
            send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, vec_bytes));
        }
    }

    // a new consumer gets connected to all producers
    fn add_consumer(&self, conn_uuid: u128, sender: ChatSender) {
        let mut mutable = self.mutable.lock();
        for (k, v) in &mutable.producers {
            send_cmd(&sender, ChatCmd::AddSink(*k, v.clone()));
        }
        mutable.consumers.insert(conn_uuid, sender);
    }

    // all consumers are told about a new producer and the producer
    // it told to add the coordinator as a sink
    // lastly, a prompt for name is sent out
    fn add_producer(&self, conn_uuid: u128, sender: ChatSender) {
        let conn_id: usize = conn_uuid.try_into().unwrap();
        let mut mutable = self.mutable.lock();
        for v in mutable.consumers.values() {
            send_cmd(v, ChatCmd::AddSink(conn_uuid, sender.clone()));
        }
        send_cmd(
            &sender,
            ChatCmd::AddSink(conn_uuid, self.my_sender.lock().as_ref().unwrap().clone()),
        );
        mutable.producers.insert(conn_uuid, sender);
        if !mutable.names.contains_key(&conn_uuid) {
            if let Some(welcome) = self.kv.get(&"name_prompt".to_string()) {
                send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, welcome.as_bytes().to_vec()));
            }
        }
    }

    // received bytes from the network
    fn received_bytes(&self, conn_id: NetConnId, mut bytes: Vec<u8>) {
        let conn_uuid: u128 = conn_id.try_into().unwrap();
        let mut mutable = self.mutable.lock();
        // we don't send if we don't have a name... as long as its not empty, the first message is a name
        if let Some(name) = mutable.names.get(&conn_uuid) {
            if let Some(sender) = mutable.consumers.get(&conn_uuid) {
                let mut msg = name.clone();
                msg.extend(bytes);
                let msg = Arc::new(msg);
                send_cmd(sender, ChatCmd::NewData(conn_uuid, msg));
            }
        } else {
            // attempt to trim right side whitespace
            let mut name = loop {
                if let Some(byte) = bytes.pop() {
                    if !byte.is_ascii_whitespace() {
                        bytes.push(byte);
                        break bytes;
                    };
                } else {
                    break bytes;
                }
            };
            // if empty, prompt and try again
            if name.is_empty() {
                if let Some(welcome) = self.kv.get(&"name_prompt".to_string()) {
                    let conn_id: usize = conn_uuid.try_into().unwrap();
                    send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, welcome.as_bytes().to_vec()));
                }
            } else {
                // otherwise save the name
                name.extend(": ".bytes());
                mutable.names.insert(conn_uuid, name);
            }
        }
    }
}

impl Machine<ChatCmd> for ChatInstance {
    fn disconnected(&self) {
        log::info!("chat service coordinator disconnected");
    }
    fn receive(&self, cmd: ChatCmd) {
        // log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            ChatCmd::Instance(conn_id, sender, component) if component == "ChatConsumer" => self.add_consumer(conn_id, sender),
            ChatCmd::RemoveSink(conn_id) => self.remove_sink(conn_id),
            ChatCmd::Instance(conn_id, sender, component) if component == "ChatProducer" => self.add_producer(conn_id, sender),
            ChatCmd::NewData(conn_id, bytes) => self.new_data(conn_id, bytes),
            _ => (),
        }
    }
}

impl Machine<NetCmd> for ChatInstance {
    fn disconnected(&self) {
        log::info!("chat coordinator (NetCmd) disconnected");
    }
    fn receive(&self, cmd: NetCmd) {
        // log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            NetCmd::CloseConn(conn_id) => self.close(conn_id),
            NetCmd::RecvBytes(conn_id, bytes) => self.received_bytes(conn_id, bytes),
            NetCmd::SendReady(_conn_id, _max_bytes) => (),
            _ => log::warn!("chat coordinator received unexpected network command"),
        }
    }
}

#[cfg(test)]
mod tests {}
