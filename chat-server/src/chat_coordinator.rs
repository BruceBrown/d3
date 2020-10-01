
use super::*;
use std::collections::HashMap;
use std::net::{SocketAddr};
use network::{get_network_sender, NetCmd, NetSender};
use settings::{Settings, Coordinator, CoordinatorVariant};
use components::{ComponentInfo, ComponentSender};

///
/// Not quite as simple as the echo server, which has a pathway of Coordinate -> Consumer -> Producer -> Coordinator
/// 
/// Normally, we'd add a new component to manage routing with multiple chat rooms. However, we're going to cheap
/// out and just have a single room.
pub fn configure(settings: &Settings, components: &[ComponentInfo]) -> Result<Option<ComponentSender> ,Box<dyn Error>> {

    // ensure our service is configure in services
    if !settings.services.contains(&Service::ChatServer) {
        log::debug!("chat service is not configured");
        return Ok(None)
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(v) = c.get(&Coordinator::ChatCoordinator) {
            let coordinator = match v {
                CoordinatorVariant::SimpleTcpConfig {tcp_address, kv} if kv.is_some() => {
                    let mutable = Mutex::new(MutableCoordinatorData {kv: kv.as_ref().unwrap().clone(), .. MutableCoordinatorData::default() });
                    Some(ChatCoordinator {
                        net_sender: get_network_sender(),
                        bind_addr: tcp_address.to_string(),
                        components: components.to_owned(),
                        mutable,
                    }
                )},
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };
            if let Some(coordinator) = coordinator {
                {
                    // maybe move up and into the match
                    let mutable = coordinator.mutable.lock().unwrap();
                    if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                        log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr );
                        continue
                    }
                    if !mutable.kv.contains_key(&"name_prompt".to_string()) {
                        log::error!("chat server is missing keyword name_prompt from config");
                        continue
                    }
                }
                log::debug!("chat server selected configuration: {:#?}", v);
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                // save out network sender until we give it away during start
                m.lock().unwrap().mutable.lock().unwrap().my_sender.replace(sender);
                let sender = executor::and_connect::<_, ComponentCmd>(&m);
                return Ok(Some(sender))
            }
        }
    }
    log::warn!("chat server coordinator didn't find its configuration.");
    Ok(None)
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
    fn create_instance(&mut self, conn_id: u128, buf_size: usize, net_sender: &NetSender, components: &[ComponentInfo]) {

        // create an instance to handle the connection, the ChatInstance has two instruction sets.
        // Wire them both to the same instance.
        if self.inst_chat_sender.is_none() {
            // get the instance running
            let (instance, sender) = executor::connect::<_,ChatCmd>(ChatInstance::new(conn_id, std::mem::take(&mut self.kv), buf_size, net_sender.clone()));
            // save the chat sender in the coordinator
            self.inst_chat_sender.replace(sender.clone());
            // save the chat sender in the instance
            instance.lock().unwrap().my_sender.lock().unwrap().replace(sender);
            // create a net sender for the instance
            let sender = executor::and_connect::<_,NetCmd>(&instance);
            // save the sender in the instance
            self.inst_net_sender.replace(sender);
        }
        // tell the network that the coordinator will handle the connection traffic
        let sender = self.inst_net_sender.as_ref().unwrap().clone();
        send_cmd(net_sender, NetCmd::BindConn(conn_id, sender));
        // let all of the components know that there's a new session for the chat service and how to contact the coordinator
        let sender = self.inst_chat_sender.as_ref().unwrap().clone();
        components.iter().for_each(|c| send_cmd(&c.sender, ComponentCmd::NewSession(conn_id, Service::ChatServer, Arc::new(sender.clone()))));
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
    fn create_instance(&self, conn_id: u128, buf_size: usize) {
        let mut mutable = self.mutable.lock().unwrap();
        mutable.create_instance(conn_id, buf_size, &self.net_sender, &self.components);
    }
}

impl Machine<ComponentCmd> for ChatCoordinator {
    fn receive(&self, cmd: ComponentCmd) {
        let mut mutable = self.mutable.lock().unwrap();
        match cmd {
            ComponentCmd::Start => {
                let my_sender = mutable.my_sender.take();
                send_cmd(&self.net_sender, NetCmd::BindListener(self.bind_addr.to_string(), my_sender.unwrap()));
            }
            ComponentCmd::Stop => {
                mutable.inst_net_sender.take();
                mutable.inst_chat_sender.take();
            }
            _=> (),
        }
    }
}

impl Machine<NetCmd> for ChatCoordinator {
    fn disconnected(&self) {
        log::warn!("chat coordinator component disconnected");
    }
    fn receive(&self, cmd: NetCmd) {
        //log::trace!("echo instance {:#?}", &cmd);
        // for a single match, this is cleaner
        if let NetCmd::NewConn(conn_id, _bind_addr, _remote_addr, buf_size) = cmd {
            self.create_instance(conn_id, buf_size);
        }
    }
}

//
// We have the echo instance, which interfaces with the network.
// We're going to hear back from 2 instances, one is consumer souce/sink
// the other is a producer source/sink. The echo instance is going to wire
// things up such that network traffic flows into the echo instance,
// from there if flows into the consumer. The consumer flows it into
// the producer and finally the producer flows it into the controller
// where it is written out. Why so complex? well, later we'll change
// one thing to turn it into a chat server.

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
        Self { conn_id, kv, buf_size, net_sender, my_sender: Mutex::default(), mutable: Mutex::new(MutableData::default()) }
    }

    // notification that a connection has closed. let everyone know that its gone
    fn close(&self, conn_id: u128) {
        // we only do a fast drop for connections that never fully connected 
        let mut fast_drop = true;
        log::debug!("coordinator notifed that connection {} is gone", conn_id);
        if let Some(gone) = self.kv.get(&"gone_message".to_string()) {
            let mutable = self.mutable.lock().unwrap();
            if let Some(name) = mutable.names.get(&conn_id) {
                if let Some(sender) = mutable.consumers.get(&conn_id) {
                    fast_drop = false;
                    let mut msg = name.clone();
                    msg.extend(gone.as_bytes().to_vec());
                    // send out the leave notification
                    send_cmd(sender, ChatCmd::NewData(conn_id, Arc::new(msg)));
                }
            }
            if !fast_drop {
                // let everyone know that the connection is gone 
                for sender in mutable.consumers.values() {
                    send_cmd(sender, ChatCmd::RemoveSink(conn_id));
                }
            }
        }
        if fast_drop {
            self.remove_sink(conn_id);
        }
    }

    // final removal of the connection, this will cause the consumer and producer to die as well
    fn remove_sink(&self, conn_id: u128) {
        // we've come full circle from where we told the consumers that the connection is closed.
        log::debug!("coordinator is dropping name, consumer and producer for connection {}", conn_id);
        let mut mutable = self.mutable.lock().unwrap();
        mutable.names.remove(&conn_id);
        mutable.producers.remove(&conn_id);
        mutable.consumers.remove(&conn_id);
    }

    // new data has arrived, we don't send to the network unless fully connected 
    fn new_data(&self, conn_id: u128, bytes: Data) {
        let vec_bytes: Vec<u8> = match Arc::try_unwrap(bytes) {
            Ok(v) => v,
            Err(v) => v.to_vec(),
        };
        let mutable = self.mutable.lock().unwrap();
        // no peeking without first giving a name
        if mutable.names.contains_key(&conn_id) {
            log::debug!("coordinator sending to {}", conn_id);
            send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, vec_bytes));
        }
    }

    // a new consumer gets connected to all producers
    fn add_consumer(&self, conn_id: u128, sender: ChatSender) {
        let mut mutable = self.mutable.lock().unwrap();
        for (k,v) in &mutable.producers {
            send_cmd(&sender, ChatCmd::AddSink(*k, v.clone()));
        }
        mutable.consumers.insert(conn_id, sender);
    }

    // all consumers are told about a new producer and the producer
    // it told to add the coordinator as a sink
    // lastly, a prompt for name is sent out
    fn add_producer(&self, conn_id: u128, sender: ChatSender) {
        let mut mutable = self.mutable.lock().unwrap();
        for v in mutable.consumers.values() {
            send_cmd(&v, ChatCmd::AddSink(conn_id, sender.clone()));
        }
        send_cmd(&sender, ChatCmd::AddSink(conn_id, self.my_sender.lock().unwrap().as_ref().unwrap().clone()));
        mutable.producers.insert(conn_id, sender);
        if !mutable.names.contains_key(&conn_id) {
            if let Some(welcome) = self.kv.get(&"name_prompt".to_string()) {
                send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, welcome.as_bytes().to_vec()));
            }
        }
    }

    // received bytes from the network
    fn received_bytes(&self, conn_id: u128, mut bytes: Vec<u8>) {
        let mut mutable = self.mutable.lock().unwrap();
        // we don't send if we don't have a name... as long as its not empty, the first message is a name
        if let Some(name) = mutable.names.get(&conn_id) {
            if let Some(sender) = mutable.consumers.get(&conn_id) {
                let mut msg = name.clone();
                msg.extend(bytes);    
                let msg = Arc::new(msg);
                send_cmd(&sender, ChatCmd::NewData(conn_id, msg));
            }
        }
        else {
            // attempt to trim right side whitespace
            let mut name = loop { if let Some(byte) = bytes.pop(){
                if !byte.is_ascii_whitespace() { bytes.push(byte); break bytes; };
            } else { break bytes } };
            // if empty, prompt and try again
            if name.is_empty() {
                if let Some(welcome) = self.kv.get(&"name_prompt".to_string()) {
                    send_cmd(&self.net_sender, NetCmd::SendBytes(conn_id, welcome.as_bytes().to_vec()));
                }
            } else {
                // otherwise save the name
                name.extend(": ".bytes());
                mutable.names.insert(conn_id, name);
            }
        }
    }
}

impl Machine<ChatCmd> for ChatInstance {
    fn disconnected(&self) {
        log::info!("chat server coordinator disconnected");
    }
    fn receive(&self, cmd: ChatCmd) {
        //log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            ChatCmd::Instance(conn_id, sender, settings::Component::ChatConsumer) => self.add_consumer(conn_id, sender),
            ChatCmd::RemoveSink(conn_id) => self.remove_sink(conn_id),
            ChatCmd::Instance(conn_id, sender, settings::Component::ChatProducer) => self.add_producer(conn_id, sender),
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
        //log::trace!("echo instance {:#?}", &cmd);
        match cmd {
            NetCmd::CloseConn(conn_id) => self.close(conn_id),
            NetCmd::RecvBytes(conn_id, bytes) => self.received_bytes(conn_id, bytes),
            NetCmd::SendReady(_conn_id, _max_bytes) => (),
            _ => log::warn!("chat coordinator received unexpected network command"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
    }
}