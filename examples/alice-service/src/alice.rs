//! This is the Alice Service. In testing, I've used Alice quite a bit. She's now going to be made
//! available to a nearby browser.
//!
//! The Alice Service illustrates a coordinator receiving connection announcments. In response,
//! a machine is created. The network is then asked to bind the connection to the created machine. That
//! machine implements a trivial http server connection. It handles receiving network bytes, sending
//! network bytes, receiving a notification that the connection is dead, and telling the network to
//! disconnect the client.
//!
//! ## Caveats
//! * The number of connections isn't tracked, so this is a great vector for overloading the server.
//! * This is just safe enough to get a server up and running and illustrate the serice.

use super::*;

// The AliceCoordinator receives notification of new connections and in response
// spawns a Alice machine to handle the connection.
#[derive(Debug)]
struct AliceCoordinator {
    // A net_sender is injected, simplifying communincation with the network.
    net_sender: NetSender,
    // The bind_addr is also injected
    bind_addr: String,
}

// The implementation of the AliceCoordinator only needs to handle a new connection
impl AliceCoordinator {
    // When a new connection arives, create Alice and let her handle the traffic
    fn add_connection(&self, conn_id: NetConnId) {
        // create Alice and let her handle connection traffic
        let (_, sender) = executor::connect::<_, NetCmd>(Alice::new(conn_id, self.net_sender.clone()));
        // give Alice's sender to the network binding her to the connection
        self.net_sender
            .send(NetCmd::BindConn(conn_id, sender))
            .expect("failed to send BindConn");
    }
}

// The AliceCoordinator implements the NetCmd instruction set. The connector only needs to
// handle the NewConn instruction.
impl Machine<NetCmd> for AliceCoordinator {
    fn receive(&self, cmd: NetCmd) {
        // normally, you'd do some matching, but since its just 1 case, a let works better
        if let NetCmd::NewConn(conn_id, to_addr, from_addr, _) = cmd {
            log::debug!("received connection [from={}, to={}, conn_id={}]", to_addr, from_addr, conn_id);
            self.add_connection(conn_id);
        }
    }
}

// The AliceCoordinator, as a coordinator, must implement the ComponentCmd instruction set.
// However, in this case, there's really nothing to do.
impl Machine<ComponentCmd> for AliceCoordinator {
    fn receive(&self, _cmd: ComponentCmd) {}
}

// The AliceState enumerates the states for Alice
#[derive(Debug, Copy, Clone, Eq, PartialEq, SmartDefault)]
enum AliceState {
    #[default]
    SendForm,
    Init,
    Start,
    Stop,
    Bye,
}

// The Alice structure is created each time there is a new connection.
#[derive(Debug)]
struct Alice {
    uuid: AtomicCell<Uuid>,
    net_sender: NetSender,
    conn_id: NetConnId,
    state: AtomicCell<AliceState>,
    logtag: String,
}

// Alice implements the NetCmd instruction set.
impl Machine<NetCmd> for Alice {
    // connected doesn't require an impl, we'll just save the uuid and log
    fn connected(&self, uuid: Uuid) {
        self.uuid.store(uuid);
        log::debug!("{} {} has entered the building", self.logtag, self.uuid.load());
    }
    // disconnected doesn't require an impl, we'll just log
    fn disconnected(&self) {
        log::debug!("{} {} has left the building", self.logtag, self.uuid.load());
    }
    // receive is required
    fn receive(&self, cmd: NetCmd) {
        match cmd {
            NetCmd::RecvBytes(_conn_id, bytes) => self.parse(bytes),
            NetCmd::CloseConn(_conn_id) => log::debug!("{} closed by remote", self.logtag),
            NetCmd::SendReady(_conn_id, bytes_available) => log::debug!("{} can send {} more bytes", self.logtag, bytes_available),
            _ => (),
        }
    }
}

// Most of Alice's implementation is dealing with http and html.
impl Alice {
    // create Alice
    fn new(conn_id: NetConnId, net_sender: NetSender) -> Self {
        Self {
            uuid: AtomicCell::new(Uuid::default()),
            net_sender,
            conn_id,
            state: AtomicCell::new(AliceState::default()),
            logtag: format!("Alice({})", conn_id),
        }
    }

    fn is_started(&self) -> bool { self.state.load() == AliceState::Start }
    fn is_stopped(&self) -> bool { self.state.load() == AliceState::Stop }
    fn is_send_form(&self) -> bool { self.state.load() == AliceState::SendForm }

    // This isn't the best of parsers, but its good enough
    fn parse(&self, bytes: Vec<u8>) {
        let byte_buffer = std::str::from_utf8(bytes.as_slice()).unwrap();
        log::trace!("{} received: {} bytes {}", self.logtag, byte_buffer.len(), byte_buffer);

        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        let res = req.parse(bytes.as_slice()).unwrap();
        if let httparse::Status::Complete(req_len) = res {
            if req.method.is_none() {
                return;
            }
            let body = std::str::from_utf8(&bytes[req_len ..]).unwrap();
            let method = req.method.take().unwrap();
            // broswsers love to ask for all kinds of stuff, we'll not allow it
            if let Some(path) = req.path {
                if path != "/" {
                    log::debug!("{} received path={}, responding 404", self.logtag, path);
                    self.send_zero_len_response("404 Not Found");
                    return;
                }
            }
            // periodic get is a keep-alive
            match method {
                "GET" if self.is_send_form() => self.send_response(None),
                "GET" => self.send_zero_len_response("200 OK"),
                "POST" => match body {
                    "Alice=Init" => self.send_response(Some(AliceState::Init)),
                    "Alice=Start" => self.send_response(Some(AliceState::Start)),
                    "Alice=Stop" => self.send_response(Some(AliceState::Stop)),
                    "Alice=Bye" => self.send_response(Some(AliceState::Bye)),
                    _ => (),
                },
                _ => (),
            }
        }
    }

    // Based upon current state and the proposed new state, send a response
    fn send_response(&self, new_state: Option<AliceState>) {
        match new_state {
            None => self.send_form(),
            Some(AliceState::Init) => self.send_already_init(),
            Some(AliceState::Start) if self.is_started() => self.send_is_started(),
            Some(AliceState::Start) => self.send_will_start(),
            Some(AliceState::Stop) if self.is_stopped() => self.send_is_stopped(),
            Some(AliceState::Stop) => self.send_will_stop(),
            Some(AliceState::Bye) => self.send_bye_and_disconnect(),
            _ => (),
        }
    }

    // Send form, with updated text.
    fn send_already_init(&self) { self.send_new_form("Well, I'm already initialized, so no, I won't do that again."); }

    // Send form, with updated text.
    fn send_is_started(&self) { self.send_new_form("Hey, I'm already started, try to stop me."); }

    // Send form, with updated text and change Alice's state to Start
    fn send_will_start(&self) {
        self.send_new_form("I'm Alice, and I've just been started.");
        self.state.store(AliceState::Start);
    }

    // Send form, with updated text.
    fn send_is_stopped(&self) { self.send_new_form("Hey, I'm already stopped, try to start me."); }

    // Send form, with updated text and change Alice's state to Stop
    fn send_will_stop(&self) {
        self.send_new_form("I'm Alice, and I've just been stopped.");
        self.state.store(AliceState::Stop);
    }

    // respond to the bye. Send BYE html as the body.
    fn send_bye_and_disconnect(&self) {
        self.send_body(BYE.to_string());
        self.state.store(AliceState::Bye);
        // tell the network to close the connection, this will also disconnect Alice
        log::debug!("{} diconnecting from the remote", self.logtag);
        if self.net_sender.send(NetCmd::CloseConn(self.conn_id)).is_err() {
            log::warn!("{} failed to send close", self.logtag);
        }
    }

    // Send the initial form
    fn send_form(&self) {
        self.send_new_form("Hello, I'm Alice, I've just been initialized, select an option.");
        self.state.store(AliceState::Init);
    }

    // Assemble a body, composed of the 2 part form with the message between the two parts
    fn send_new_form(&self, msg: &str) {
        let complete_form = format!("{}{}{}", FORM_PART_1, msg, FORM_PART_2);
        self.send_body(complete_form);
    }

    // Send a response that is just a 200 OK, 404 Not Found, etc
    fn send_zero_len_response(&self, code_and_text: &str) {
        let http_resp = format!("HTTP/1.1 {}\r\nContent-Length:0\r\n\r\n", code_and_text);
        self.send_network_bytes(http_resp.as_bytes().to_vec());
    }

    // Assemble a 200 OK response that includes a body
    fn send_body(&self, body: String) {
        // convert body and use the length for the content length
        let bytes = body.as_bytes().to_vec();
        // send ok with length of body
        let form_ok = format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\n\r\n", bytes.len());
        self.send_network_bytes(form_ok.as_bytes().to_vec());
        // send the body
        self.send_network_bytes(bytes);
    }

    fn send_network_bytes(&self, bytes: Vec<u8>) {
        if self.net_sender.send(NetCmd::SendBytes(self.conn_id, bytes)).is_err() {
            log::warn!("{} failed to send response", self.logtag);
        }
    }
}

// The ugliest part of a coordinator is the configuration parsing.
pub fn configure(settings: &Settings, _components: &[ComponentInfo]) -> Result<Option<ComponentSender>, ComponentError> {
    // ensure our service is configure in services
    if !settings.services.contains("AliceService") {
        log::debug!("alice service is not configured");
        return Ok(None);
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(value) = c.get("AliceCoordinator") {
            let maybe_coordinator = match value {
                CoordinatorVariant::SimpleTcpConfig { tcp_address, .. } => Some(AliceCoordinator {
                    net_sender: get_network_sender(),
                    bind_addr: tcp_address.to_string(),
                }),
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };

            if let Some(coordinator) = maybe_coordinator {
                if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                    log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr);
                    continue;
                }
                log::debug!("alice service selected configuration: {:#?}", value);
                // create the AliceCoordinator machine with a NetCmd communication channel
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                // bind the server address to the coordinator
                get_network_sender()
                    .send(NetCmd::BindListener(m.lock().unwrap().bind_addr.clone(), sender))
                    .expect("BindListener failed");
                // add a ComponentCmd communication channel to the coordinator
                let sender = executor::and_connect::<_, ComponentCmd>(&m);
                return Ok(Some(sender));
            }
        }
    }
    log::warn!("The configuration for the monitor service coordinator was not found.");
    Err(ComponentError::BadConfig(
        "The configuration for the monitor service coordinator was not found.".to_string(),
    ))
}

// Our simple form, in two parts. Usually, they are assembled with some text between them.
const FORM_PART_1: &str = "<!DOCTYPE html>\n\
<html>\n\
<body>\n\
<FORM action=\"http://127.0.0.1:8080\" method=\"POST\">\n\
<P>\n";
const FORM_PART_2: &str = "\n\
<P>\n\
<INPUT type=\"radio\" name=\"Alice\" value=\"Init\"> Initialize Alice<BR>\n\
<INPUT type=\"radio\" name=\"Alice\" value=\"Start\"> Start Alice<BR>\n\
<INPUT type=\"radio\" name=\"Alice\" value=\"Stop\"> Stop Alice<BR>\n\
<INPUT type=\"radio\" name=\"Alice\" value=\"Bye\"> Wave good-bye to Alice<BR>\n\
<INPUT type=\"submit\" value=\"Send\"> <INPUT type=\"reset\">\n\
</P>\n\
</FORM>\n\
</body>\n\
</html>\n";

const BYE: &str = "<!DOCTYPE html>\n\
<html>\n\
<body>\n\
 <P>\n\
Alice has left the building.\n\
</body>\n\
</html>\n";
