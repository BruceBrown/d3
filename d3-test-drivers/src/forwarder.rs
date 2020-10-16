use super::*;

/// This is the Forwarder, the Swiss Army Knife of machines. It implements the TestMessage instruction set.
/// This allows it to be an accumulator of messages (fanin), a distributor of messages (fanout), a forwarder,
/// a notifier, and for extra credit, it can randomly forward a mutated message (chaos-monkey). It illustrates
/// how a fairly simple machine can be configured by an instruction set to act in different roles. It
/// also illustrates pipeling, where an instruction is tranformed and then sent to another machine for
/// additional transformations.
#[derive(Default)]
pub struct Forwarder {
    /// a id, mosly used for logging
    id: usize,
    /// received_count is the count of messages received by this forwarder.
    received_count: AtomicUsize,
    /// send_count is the count of messages sent by this forwarder.
    send_count: AtomicUsize,
    /// The mutable bits...
    mutable: Mutex<ForwarderMutable>,
}

/// This is the mutable part of the Forwarder
#[derive(SmartDefault)]
pub struct ForwarderMutable {
    /// collection of senders, each will be sent any received message.
    senders: Vec<TestMessageSender>,
    /// notify_count is compared against received_count for means of notifcation.
    notify_count: usize,
    /// notify_sender is sent a TestData message with the data being the number of messages received.
    notify_sender: Option<TestMessageSender>,
    /// forwarding multiplier
    #[default = 1]
    forwarding_multiplier: usize,
    // Chaos monkey random
    #[default(Uniform::from(0..1))]
    range: Uniform<usize>,
    rng: rand::rngs::OsRng,
}
impl ForwarderMutable {
    /// get an index suitable for obtaining a random sender from the senders vector
    fn get_monkey_fwd(&mut self) -> usize { self.range.sample(&mut self.rng) }
    fn drop_all_senders(&mut self) {
        self.senders.clear();
        let _ = self.notify_sender.take();
    }
}

impl Forwarder {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
    pub const fn get_id(&self) -> usize { self.id }
    pub fn get_and_clear_received_count(&self) -> usize {
        let received_count = self.received_count.load(Ordering::SeqCst);
        self.received_count.store(0, Ordering::SeqCst);
        self.send_count.store(0, Ordering::SeqCst);
        received_count
    }
}

impl Machine<TestMessage> for Forwarder {
    fn disconnected(&self) {
        // drop senders
        self.mutable.lock().as_mut().unwrap().drop_all_senders();
    }

    fn receive(&self, message: TestMessage) {
        // it a bit ugly, but its also a clean way to handle the data we need to access
        let mut mutable = self.mutable.lock().unwrap();
        // handle configuation messages without bumping counters
        match message {
            TestMessage::Notify(sender, on_receive_count) => {
                mutable.notify_sender = Some(sender);
                mutable.notify_count = on_receive_count;
                return;
            },
            TestMessage::AddSender(sender) => {
                mutable.senders.push(sender);
                return;
            },
            TestMessage::ForwardingMultiplier(count) => {
                mutable.forwarding_multiplier = count;
                return;
            },
            TestMessage::RemoveAllSenders => {
                mutable.drop_all_senders();
                return;
            },
            _ => (),
        }
        self.received_count.fetch_add(1, Ordering::SeqCst);
        // forward the message
        match message {
            TestMessage::ChaosMonkey { .. } if message.can_advance() => {
                let idx = mutable.get_monkey_fwd();
                mutable.senders[idx].send(message.advance()).unwrap();
            },
            TestMessage::ChaosMonkey { .. } => {
                if let Some(notifier) = mutable.notify_sender.as_ref() {
                    notifier.send(TestMessage::TestData(0)).unwrap();
                }
            },
            TestMessage::TestData(_seq) => mutable.senders.iter().for_each(|sender| {
                for _ in 0 .. mutable.forwarding_multiplier {
                    let count = self.send_count.fetch_add(1, Ordering::SeqCst);
                    sender.send(TestMessage::TestData(count)).unwrap()
                }
            }),
            TestMessage::TestCallback(sender, mut test_struct) => {
                test_struct.received_by = self.id;
                sender.send(TestMessage::TestStruct(test_struct)).unwrap();
            },
            _ => mutable.senders.iter().for_each(|sender| {
                for _ in 0 .. mutable.forwarding_multiplier {
                    sender.send(message.clone()).unwrap()
                }
            }),
        };
        // send notification if we've met the criteria
        if self.received_count.load(Ordering::SeqCst) == mutable.notify_count {
            if let Some(notifier) = mutable.notify_sender.as_ref() {
                notifier
                    .send(TestMessage::TestData(self.received_count.load(Ordering::SeqCst)))
                    .expect("failed to notify");
                self.received_count.store(0, Ordering::SeqCst);
                self.send_count.store(0, Ordering::SeqCst);
            }
        }
    }
}
