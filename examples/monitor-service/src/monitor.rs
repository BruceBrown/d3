use super::*;
use std::collections::HashSet;

pub fn configure(settings: &Settings, components: &[ComponentInfo]) -> Result<Option<ComponentSender>, ComponentError> {
    // ensure our service is configure in services
    if !settings.services.contains("MonitorService") {
        log::debug!("monitor service is not configured");
        return Ok(None);
    }
    // find ourself in the list
    for c in &settings.coordinator {
        if let Some(value) = c.get("MonitorCoordinator") {
            let maybe_coordinator = match value {
                CoordinatorVariant::SimpleTcpConfig { tcp_address, .. } => Some(MonitorCoordinator {
                    net_sender: get_network_sender(),
                    bind_addr: tcp_address.to_string(),
                    components: components.to_owned(),
                    mutable: Arc::new(Mutex::new(MutableData::default())),
                }),
                #[allow(unreachable_patterns)] // in case it becomes reachable, we want to know
                _ => None,
            };
            if let Some(coordinator) = maybe_coordinator {
                if coordinator.bind_addr.parse::<SocketAddr>().is_err() {
                    log::error!("Unable to parse {} into a SocketAddr", &coordinator.bind_addr);
                    continue;
                }
                log::debug!("monitor service selected configuration: {:#?}", value);
                let (m, sender) = executor::connect::<_, NetCmd>(coordinator);
                m.lock().unwrap().mutable.lock().unwrap().set_sender(sender.clone());
                get_network_sender()
                    .send(NetCmd::BindListener(m.lock().unwrap().bind_addr.clone(), sender))
                    .expect("BindListener failed");
                let sender = executor::and_connect::<_, CoreStatsMessage>(&m);
                executor::stats::add_core_stats_sender(sender);
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

#[derive(Debug, Default)]
struct MutableData {
    my_sender: Option<NetSender>,
    connections: HashSet<NetConnId>,
}
impl MutableData {
    fn set_sender(&mut self, sender: NetSender) { self.my_sender = Some(sender); }
    fn send_bytes(&self, bytes: Vec<u8>, net_sender: &NetSender) {
        for c in &self.connections {
            if net_sender.send(NetCmd::SendBytes(*c, bytes.clone())).is_err() {}
        }
    }
}

// even though components is unused now, leave it for later use
#[allow(dead_code)]
struct MonitorCoordinator {
    net_sender: NetSender,
    bind_addr: String,
    components: Vec<ComponentInfo>,
    mutable: Arc<Mutex<MutableData>>,
}
impl MonitorCoordinator {
    fn add_connection(&self, conn_id: NetConnId) {
        if self
            .net_sender
            .send(NetCmd::BindConn(
                conn_id,
                self.mutable.lock().unwrap().my_sender.as_ref().unwrap().clone(),
            ))
            .is_err()
        {}
        self.mutable.lock().unwrap().connections.insert(conn_id);
    }
    fn remove_connection(&self, conn_id: NetConnId) { self.mutable.lock().unwrap().connections.remove(&conn_id); }
    fn executor_stats(&self, stats: ExecutorStats) {
        let bytes: Vec<u8> = format!("{:#?}", stats).as_bytes().to_vec();
        self.mutable.lock().unwrap().send_bytes(bytes, &self.net_sender);
    }
    fn sched_stats(&self, stats: SchedStats) {
        let bytes: Vec<u8> = format!("{:#?}", stats).as_bytes().to_vec();
        self.mutable.lock().unwrap().send_bytes(bytes, &self.net_sender);
    }
}

impl Machine<NetCmd> for MonitorCoordinator {
    fn receive(&self, cmd: NetCmd) {
        match cmd {
            NetCmd::NewConn(conn_id, _, _, _) => self.add_connection(conn_id),
            NetCmd::CloseConn(conn_id) => self.remove_connection(conn_id),
            _ => (),
        }
    }
}

impl Machine<CoreStatsMessage> for MonitorCoordinator {
    fn receive(&self, cmd: CoreStatsMessage) {
        match cmd {
            CoreStatsMessage::ExecutorStats(stats) => self.executor_stats(stats),
            CoreStatsMessage::SchedStats(stats) => self.sched_stats(stats),
        }
    }
}

impl Machine<ComponentCmd> for MonitorCoordinator {
    fn receive(&self, cmd: ComponentCmd) {
        match cmd {
            ComponentCmd::Start => (),
            ComponentCmd::Stop => (),
            _ => (),
        }
    }
}
