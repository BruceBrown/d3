use self::forwarder::Forwarder;
use super::*;

/// DaisyChain will setup a linear network of machines in which a messages
/// received by a machine is forwarded to the next. Every machine in the
/// network will be visited by the message. Send 400 message in a network
/// of 4000 machines produces 1,600,000 message propagations. Additionally,
/// the propagation through the network is essentially a single pulse wave.
/// In this case, a pulse of 400 messages.
#[derive(Debug, SmartDefault)]
pub struct DaisyChainDriver {
    #[default = 2000]
    pub machine_count: usize,

    #[default = 100]
    pub message_count: usize,

    #[default = true]
    pub bound_queue: bool,

    #[default = 1]
    pub forwarding_multiplier: usize,

    #[default(Duration::from_secs(10))]
    pub duration: Duration,

    #[default(Vec::with_capacity(4010))]
    pub senders: Vec<TestMessageSender>,

    pub first_sender: Option<TestMessageSender>,
    pub receiver: Option<TestMessageReceiver>,
    pub baseline: usize,
    pub exepected_message_count: usize,
    #[default(AtomicUsize::new(1))]
    pub iteration: AtomicUsize,
}
impl TestDriver for DaisyChainDriver {
    // setup the machines
    fn setup(&mut self) {
        self.baseline = executor::get_machine_count();
        let (_, s) = if self.bound_queue {
            executor::connect(Forwarder::new(1))
        } else {
            executor::connect_unbounded(Forwarder::new(1))
        };
        self.first_sender = Some(s.clone());
        let mut last_sender = s.clone();
        self.senders.push(s);
        for idx in 2 ..= self.machine_count {
            let (_, s) = if self.bound_queue {
                executor::connect(Forwarder::new(idx))
            } else {
                executor::connect_unbounded(Forwarder::new(idx))
            };
            last_sender.send(TestMessage::AddSender(s.clone())).unwrap();
            last_sender
                .send(TestMessage::ForwardingMultiplier(self.forwarding_multiplier))
                .unwrap();
            last_sender = s.clone();
            self.senders.push(s);
        }
        self.exepected_message_count = self.message_count * (self.forwarding_multiplier.pow((self.machine_count - 1) as u32));
        if self.forwarding_multiplier > 1 {
            log::info!("daisy_chain: expecting {} messages", self.exepected_message_count);
        }
        // turn the last into a notifier
        let (sender, receiver) = channel();
        log::info!(
            "daisy_chain: first in chain is {} final notifier is chan {}, notifier accumulator is chan {}",
            &self.first_sender.as_ref().unwrap().get_id(),
            &sender.get_id(),
            &last_sender.get_id()
        );

        last_sender.send(TestMessage::Notify(sender, self.exepected_message_count)).unwrap();
        self.receiver = Some(receiver);

        // wait for the scheduler/executor to get them all assigned
        if wait_for_machine_setup(self.baseline + self.machine_count - 1).is_err() {
            panic!("daisy_chain: machine setup failed");
        }
        log::info!("daisy_chain: setup complete");
    }

    // tear down the machines
    fn teardown(mut daisy_chain: Self) {
        log::debug!("daisy_chain: tear-down started");
        let baseline = daisy_chain.baseline;
        daisy_chain
            .first_sender
            .take()
            .unwrap()
            .send(TestMessage::RemoveAllSenders)
            .unwrap();
        daisy_chain
            .senders
            .drain(..)
            .for_each(|s| s.send(TestMessage::RemoveAllSenders).unwrap());
        daisy_chain.receiver = None;
        drop(daisy_chain);

        // wait for the machines to all go away
        if wait_for_machine_teardown("daisy_chain", baseline).is_err() {
            panic!("daisy_chain: machine tear-down failed");
        }
        log::info!("daisy_chain: tear-down complete");
    }

    // run a single iteration
    fn run(&self) {
        // let count = self.iteration.fetch_add(1, Ordering::SeqCst);
        // log::info!("daisy_chain iteration: {}", count);
        if let Some(sender) = self.first_sender.as_ref() {
            for msg_id in 0 .. self.message_count {
                sender.send(TestMessage::TestData(msg_id)).unwrap();
            }
            if let Some(receiver) = self.receiver.as_ref() {
                if wait_for_notification(receiver, self.exepected_message_count, self.duration).is_err() {
                    panic!("daisy_chain: completion notification failed");
                }
            }
        }
    }
}
