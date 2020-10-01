use super::*;

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use d3_lib::machine_impl::*;
use d3_lib::instruction_sets::{TestMessage};


struct ForwarderSettings {
    run: Vec<settings::Field>,
    default: settings::FieldMap,
    daisy_chain: Option<settings::FieldMap>,
    fanout_fanin: Option<settings::FieldMap>,
}

/// Take the setting from the variant and turn them into a concrete stuct that we can
/// pass around. Probably could do this with a trait...
/// 
pub fn run(settings: &settings::Settings) {
    log::info!("running forwarder");
    //
    // pull the forwarder info out of the additoanl hash map
    //
    for a in &settings.additional {
        if let Some(v) = a.get(&settings::Additional::Forwarder) {
            // v is a variant in AdditionalVariant, need to extract things info the Forwarder
            let f = match v.clone() {
                settings::AdditionalVariant::Forwarder {run, default, daisy_chain, fanout_fanin } => ForwarderSettings { run, default, daisy_chain, fanout_fanin },
            };
            // at this point f represents the Forwarder parameters
            for r in &f.run {
                match r {
                    settings::Field::daisy_chain => run_daisy_chain(&f),
                    settings::Field::fanout_fanin => run_fanout_fanin(&f),
                    _=> (),
                }
            }
        }
    }
}

/// This simply takes two maps, merges them, returning the merged result. In our case
/// we're taking a default map and overriding with any fields provided in the primary map
fn merge_maps(map1: settings::FieldMap, map2: settings::FieldMap) -> settings::FieldMap {
    map1.into_iter().chain(map2).collect()
}

#[derive(Debug)]
struct RunParams {
    machine_count: usize,
    messages: usize,
    iterations: usize,
    timeout: std::time::Duration,
}

// convert from a field map to RunParams
impl From<settings::FieldMap> for RunParams {
    fn from(map: settings::FieldMap) -> Self {
        Self {
            machine_count: *map
                .get(&settings::Field::machines)
                .expect("machines missing"),
            messages: *map
                .get(&settings::Field::messages)
                .expect("messages missing"),
            iterations: *map
                .get(&settings::Field::iterations)
                .expect("iterations missing"),
            timeout: std::time::Duration::from_secs(
                *map.get(&settings::Field::timeout).expect("timeout missing") as u64,
            ),
        }
    }
}

fn run_daisy_chain(settings: &ForwarderSettings) {
    let fields = match &settings.daisy_chain {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let params = RunParams::from(fields);
    log::info!("daisy_chain: {:?}", params);

    // the daisy chain sets up a chain of forwarders, then sends a message
    // into the first, which should run through all the forwarders and end
    // with a notification.

    let mut machines: Vec<TestMessageSender> = Vec::with_capacity(params.machine_count);
    let mut instances: Vec<Arc<Mutex<Forwarder>>> = Vec::with_capacity(params.machine_count);
    let f = Forwarder::new(1);
    let (_f, s) = executor::connect(f);
    instances.push(_f);
    let first_sender = s.clone();
    let mut last_sender = s.clone();
    machines.push(s);
    for idx in 2..=params.machine_count {
        let (_f, s) = executor::connect(Forwarder::new(idx));
        instances.push(_f);
        last_sender.send(TestMessage::AddSender(s.clone())).unwrap();
        last_sender = s.clone();
        machines.push(s);
    }
    // turn the last into a notifier
    let (sender, receiver) = channel();
    last_sender
        .send(TestMessage::Notify(sender, params.messages))
        .unwrap();

    // drive the forwarders...
    for _ in 0..params.iterations {
        for msg_id in 0..params.messages {
            match first_sender.try_send(TestMessage::TestData(msg_id)) {
                Ok(()) => (),
                Err(e) => match e {
                    crossbeam::TrySendError::Full(m) => {
                        log::info!("full");
                        first_sender.send(m).unwrap();
                    }
                    crossbeam::TrySendError::Disconnected(_) => {
                        log::info!("disconnected");
                    }
                },
            };
        }
        log::info!("sent an iteration, waiting for response");
        match receiver.recv_timeout(params.timeout) {
            Ok(m) => {
                assert_eq!(m, TestMessage::TestData(params.messages));
                log::info!("an iteration completed");
            }
            Err(_) => {
                for forwarder in &instances {
                    log::warn!(
                        "took too long, id {} count{}",
                        forwarder.lock().unwrap().get_id(),
                        forwarder.lock().unwrap().get_and_clear_received_count()
                    );
                }
            }
        };
    }
    log::info!("completed daisy-chain run");
    /* Enable if you want to watch cleanup
    // unnecessary, but this gives a graceful cleanup before proceeding...
    drop(machines);
    drop(first_sender);
    drop(last_sender);
    drop(receiver);
    drop(instances);
    std::thread::sleep(std::time::Duration::from_millis(1000));
    */
}

fn run_fanout_fanin(settings: &ForwarderSettings) {
    // get params, this will wipe out fields.
    let fields = match &settings.fanout_fanin {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let params = RunParams::from(fields);

    let fields = match &settings.fanout_fanin {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let fanin_capacity = *fields
        .get(&settings::Field::fanin_capacity)
        .unwrap_or(&executor::get_default_channel_capacity());
    log::info!(
        "fanout_fanin: {:?}, fanin_capacity {}",
        params, fanin_capacity
    );

    let mut machines: Vec<TestMessageSender> = Vec::with_capacity(params.machine_count);
    let (_f, fanout_sender) = executor::connect(Forwarder::new(1));
    let (fanin, fanin_sender) = executor::connect_with_capacity(Forwarder::new(2), fanin_capacity);

    for idx in 3..=params.machine_count {
        let (_f, s) = executor::connect(Forwarder::new(idx));
        fanout_sender
            .send(TestMessage::AddSender(s.clone()))
            .unwrap();
        s.send(TestMessage::AddSender(fanin_sender.clone()))
            .unwrap();
        machines.push(s);
    }
    // turn the fanin into a notifier
    let (sender, receiver) = channel();
    let expect_count = (params.machine_count - 2) * params.messages;
    fanin_sender
        .send(TestMessage::Notify(sender, expect_count))
        .unwrap();
    // give things a chance to complete setup before we start driving messages
    std::thread::sleep(std::time::Duration::from_millis(50));

    for _ in 0..params.iterations {
        for _ in 0..params.messages {
            fanout_sender.send(TestMessage::Test).unwrap();
        }
        match receiver.recv_timeout(params.timeout) {
            Ok(m) => {
                assert_eq!(m, TestMessage::TestData(expect_count));
                log::info!("an iteration completed");
            }
            Err(e) => {
                log::info!("error {}", e);
                log::info!(
                    "fanin received: {} messages",
                    fanin.lock().unwrap().get_and_clear_received_count()
                );
                return;
            }
        };
    }
}


type TestMessageSender = Sender<TestMessage>;
/// The Forwarder is the swiss army knife for tests. It can be a fanin, fanout, chain, or callback receiver.
#[derive(Default)]
pub struct Forwarder {
    id: usize,
    /// received_count is the count of messages received by this forwarder.
    received_count: AtomicUsize,
    /// The mutable bits...
    mutations: Mutex<ForwarderMutations>,
}

#[derive(Default)]
pub struct ForwarderMutations {
    /// collection of senders, each will be sent any received message.
    senders: Vec<TestMessageSender>,
    /// notify_count is compared against received_count for means of notifcation.
    notify_count: usize,
    /// notify_sender is sent a TestData message with the data being the number of messages received.
    notify_sender: Option<TestMessageSender>,
    /// sequencing
    sequence: usize,
}

impl Forwarder {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
    pub fn get_id(&self) -> usize {
        self.id
    }
    pub fn get_and_clear_received_count(&self) -> usize {
        let received_count = self.received_count.load(Ordering::SeqCst);
        self.received_count.store(0, Ordering::SeqCst);
        received_count
    }
}

impl Machine<TestMessage> for Forwarder {
    fn disconnected(&self) {
        // drop senders
        let mut mutations = self.mutations.lock().unwrap();
        mutations.senders.clear();
        let sender = mutations.notify_sender.take();
        drop(sender);
    }

    fn receive(&self, message: TestMessage) {
        // it a bit ugly, but its also a clean way to handle the data we need to access
        let mut mutations = self.mutations.lock().unwrap();
        // handle configuation messages without bumping counters
        match message {
            TestMessage::Notify(sender, on_receive_count) => {
                mutations.notify_sender = Some(sender);
                mutations.notify_count = on_receive_count;
                return;
            }
            TestMessage::AddSender(sender) => {
                mutations.senders.push(sender);
                return;
            }
            _ => (),
        }
        self.received_count.fetch_add(1, Ordering::SeqCst);
        // forward the message
        mutations
            .senders
            .iter()
            .for_each(|sender| sender.send(message.clone()).unwrap());
        if mutations.notify_count > 0 && (self.received_count.load(Ordering::SeqCst) % 10000 == 0) {
            log::info!(
                "rcvs {} out of {}",
                self.received_count.load(Ordering::SeqCst),
                mutations.notify_count
            );
        }
        // send notification if we've met the criteria
        if self.received_count.load(Ordering::SeqCst) == mutations.notify_count
            && mutations.notify_sender.is_some()
        {
            mutations
                .notify_sender
                .as_ref()
                .unwrap()
                .send(TestMessage::TestData(
                    self.received_count.load(Ordering::SeqCst),
                ))
                .unwrap();
            self.received_count.store(0, Ordering::SeqCst);
        }
        match message {
            TestMessage::TestCallback(sender, mut test_struct) => {
                test_struct.received_by = self.id;
                sender.send(TestMessage::TestStruct(test_struct)).unwrap();
            }
            TestMessage::TestData(seq) => {
                if seq == 0 as usize {
                    mutations.sequence = 0
                } else {
                    mutations.sequence += 1;
                    assert_eq!(seq, mutations.sequence);
                }
            }
            _ => (),
        }
    }
}

impl fmt::Display for Forwarder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Forwarder {} forwarded {}",
            self.id,
            self.received_count.load(Ordering::SeqCst)
        )
    }
}
