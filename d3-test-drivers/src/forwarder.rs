use super::*;
use rand::prelude::*;

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
    /// The mutable bits...
    mutable: Mutex<ForwarderMutable>,
}

/// This is the mutable part of the Forwarder
#[derive(SmartDefault)]
pub struct ForwarderMutable {
    /// collection of senders, each will be sent any received message.
    senders: Vec<TestMessageSender>,
    /// received_count is the count of messages received by this forwarder.
    received_count: usize,
    /// send_count is the count of messages sent by this forwarder.
    send_count: usize,
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
    // for TestData, this is the next in sequence
    next_seq: usize,
}
impl ForwarderMutable {
    /// get an index suitable for obtaining a random sender from the senders vector
    fn get_monkey_fwd(&mut self) -> usize {
        let mut rng = thread_rng();
        self.range.sample(&mut rng)
    }
    fn drop_all_senders(&mut self) {
        self.senders.clear();
        self.notify_sender = None;
    }

    /// if msg is TestData, validate the sequence or reset if 0
    fn validate_sequence(&mut self, msg: TestMessage) -> Result<TestMessage, TestMessage> {
        match msg {
            TestMessage::TestData(seq) if seq == self.next_seq => self.next_seq += 1,
            TestMessage::TestData(seq) if seq == 0 => self.next_seq = 1,
            TestMessage::TestData(_) => return Err(msg),
            _ => (),
        }
        // bump received count
        self.received_count += 1;
        Ok(msg)
    }

    /// If msg is a configuration msg, handle it otherwise return it as an error
    fn handle_config(&mut self, msg: TestMessage) -> Result<(), TestMessage> {
        match msg {
            TestMessage::Notify(sender, on_receive_count) => {
                self.notify_sender = Some(sender);
                self.notify_count = on_receive_count;
            },
            TestMessage::AddSender(sender) => {
                self.senders.push(sender);
                self.range = Uniform::from(0 .. self.senders.len());
            },
            TestMessage::AddSenders(senders) => {
                self.senders = senders;
                self.range = Uniform::from(0 .. self.senders.len());
            },
            TestMessage::ForwardingMultiplier(count) => self.forwarding_multiplier = count,
            TestMessage::RemoveAllSenders => self.drop_all_senders(),
            msg => return Err(msg),
        }
        Ok(())
    }

    /// handle the action messages
    fn handle_action(&mut self, message: TestMessage, id: usize) {
        match message {
            TestMessage::ChaosMonkey { .. } if message.can_advance() => {
                let idx = self.get_monkey_fwd();
                self.senders[idx].send(message.advance()).unwrap();
            },
            TestMessage::ChaosMonkey { .. } => {
                if let Some(notifier) = self.notify_sender.as_ref() {
                    notifier.send(TestMessage::TestData(0)).unwrap();
                }
            },
            TestMessage::TestData(_) => {
                for sender in &self.senders {
                    for _ in 0 .. self.forwarding_multiplier {
                        sender.send(TestMessage::TestData(self.send_count)).unwrap();
                        self.send_count += 1;
                    }
                }
            },
            TestMessage::TestCallback(sender, mut test_struct) => {
                test_struct.received_by = id;
                sender.send(TestMessage::TestStruct(test_struct)).unwrap();
            },
            _ => self.senders.iter().for_each(|sender| {
                for _ in 0 .. self.forwarding_multiplier {
                    sender.send(message.clone()).unwrap();
                }
            }),
        }
    }

    /// handle sending out a notification and resetting counters when notificaiton is sent
    fn handle_notification(&mut self) {
        if self.received_count == self.notify_count {
            let count = self.get_and_clear_received_count();
            // log::trace!("received {} out of {}", count, mutable.notify_count);
            if let Some(notifier) = self.notify_sender.as_ref() {
                notifier.send(TestMessage::TestData(count)).expect("failed to notify");
            }
        }
    }

    /// get the current received count and clear counters
    fn get_and_clear_received_count(&mut self) -> usize {
        let received_count = self.received_count;
        self.received_count = 0;
        self.send_count = 0;
        received_count
    }
}

impl Forwarder {
    pub fn new(id: usize) -> Self { Self { id, ..Default::default() } }
    pub const fn get_id(&self) -> usize { self.id }
    pub fn get_and_clear_received_count(&self) -> usize { self.mutable.lock().get_and_clear_received_count() }
}

impl Machine<TestMessage> for Forwarder {
    fn disconnected(&self) {
        // drop senders
        self.mutable.lock().drop_all_senders();
    }

    fn receive(&self, message: TestMessage) {
        // it a bit ugly, but its also a clean way to handle the data we need to access
        let mut mutable = self.mutable.lock();
        match mutable.handle_config(message) {
            Ok(_) => (),
            Err(msg) => match mutable.validate_sequence(msg) {
                Ok(msg) => {
                    mutable.handle_action(msg, self.id);
                    mutable.handle_notification();
                },
                Err(msg) => panic!("sequence error fwd {}, msg {:#?}", self.id, msg),
            },
        }
    }
}
