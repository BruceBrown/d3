use super::*;

// The EchoCoordinator receives notification of new packet and in response
// sends the packet back to the sender
#[derive(Debug)]
struct EchoCoordinator {
    // A net_sender is injected, simplifying communincation with the network.
    net_sender: NetSender,
    // The bind_addr is also injected
    bind_addr: String,
}

// The EchoCoordinator implements the NetCmd instruction set. The connector only needs to
// handle the RecvPkt instruction.
impl Machine<NetCmd> for EchoCoordinator {
    fn receive(&self, cmd: NetCmd) {
        // normally, you'd do some matching, but since its just 1 case, a let works better
        if let NetCmd::RecvPkt(conn_id, _local_addr, remote_addr, bytes) = cmd {
            // log::debug!("received pkt [from={}, to={}, bytes={:#?}]", remote_addr, local_addr, bytes);
            let cmd = NetCmd::SendPkt(conn_id, remote_addr.to_string(), bytes);
            if self.net_sender.send(cmd).is_err() {
                log::warn!("failed to echo udp packet back to {}", remote_addr)
            }
        }
    }
}

// The EchoCoordinator, as a coordinator, must implement the ComponentCmd instruction set.
// However, in this case, there's really nothing to do.
impl Machine<ComponentCmd> for EchoCoordinator {
    fn receive(&self, _cmd: ComponentCmd) {}
}

// The ugliest part of a coordinator is the configuration parsing.
pub fn configure(settings: &Settings, _components: &[ComponentInfo]) -> Result<Option<ComponentSender>, ComponentError> {
    // ensure our service is configure in services
    if !settings.services.contains("UdpEchoService") {
        log::debug!("udp echo service is not configured");
        return Ok(None);
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(value) = c.get("UdpEchoCoordinator") {
            let maybe_coordinator = match value {
                CoordinatorVariant::SimpleUdpConfig { udp_address, .. } => Some(EchoCoordinator {
                    net_sender: get_network_sender(),
                    bind_addr: udp_address.to_string(),
                }),
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };

            if let Some(coordinator) = maybe_coordinator {
                if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                    log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr);
                    continue;
                }
                log::debug!("udp echo service selected configuration: {:#?}", value);
                // create the coordinator machine with a NetCmd communication channel
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                // bind the server address to the coordinator
                get_network_sender()
                    .send(NetCmd::BindUdpListener(m.bind_addr.clone(), sender))
                    .expect("BindListener failed");
                // add a ComponentCmd communication channel to the coordinator
                let sender = executor::and_connect::<_, ComponentCmd>(&m);
                return Ok(Some(sender));
            }
        }
    }
    log::warn!("The configuration for the udp echo service coordinator was not found.");
    Err(ComponentError::BadConfig(
        "The configuration for the udp echo service coordinator was not found.".to_string(),
    ))
}
