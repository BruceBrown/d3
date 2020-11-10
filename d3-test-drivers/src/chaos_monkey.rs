use self::forwarder::Forwarder;
use super::*;

/// ChaosMonkey will setup a network of machines in which a message received by any machine
/// can be forwarded to any other machine, including itself. To break the cyclic nature of this,
/// the message has a counter, inflection, and mutation (increment or decrement). The counter
/// starts with a value of 0 and a mutation of increment, as such each time it is forwarded,
/// the counter is incremented until it reaches the infection, at which point the mutation
/// is changed to decrement. The counter will decrement until it reaches 0, at which point
/// it will cease to be forwarded and a notification will be sent indicating that the
/// message forwarding for that message is complete. When all messages reach 0, the test
/// is complete.
///
/// For example, if the inflection is 3, the message with fwd 0, 1, 2, 3, 3, 2, 1, 0.
/// If there are 400 messages, and an inflection of 9 20x400 messages will be propagated.
/// This is purely random, if you have 4000 machines, in this scenerio each machine would be
/// visted by ~2 messages.
///
/// The message count represents concurrent number of messages flowing through the machines,
/// while the inflection value represents the lifetime of the message. Varing the machine count
/// varies the number of messages a machine may receive.
#[derive(Debug, SmartDefault)]
pub struct ChaosMonkeyDriver {
    #[default = 2000]
    pub machine_count: usize,

    #[default = 100]
    pub message_count: usize,

    #[default = 29]
    pub inflection_value: u32,

    #[default = true]
    pub bound_queue: bool,

    #[default(Duration::from_secs(10))]
    pub duration: Duration,

    #[default(Vec::with_capacity(2010))]
    pub senders: Vec<TestMessageSender>,
    pub receiver: Option<TestMessageReceiver>,
    pub baseline: usize,
}

impl TestDriver for ChaosMonkeyDriver {
    // setup the machines
    fn setup(&mut self) {
        self.baseline = executor::get_machine_count();
        // we're going to create N machines, each having N senders, plus a notifier.
        for idx in 1 ..= self.machine_count {
            let (_f, s) = if self.bound_queue {
                executor::connect(Forwarder::new(idx))
            } else {
                executor::connect_unbounded(Forwarder::new(idx))
            };
            self.senders.push(s);
        }
        let (_, notifier) = if self.bound_queue {
            executor::connect(Forwarder::new(self.machine_count + 1))
        } else {
            executor::connect_unbounded(Forwarder::new(self.machine_count + 1))
        };
        log::debug!("chaos_monkey: monkeys created");
        // form a complete map by sending all the monkey's senders to each monkey
        for s1 in &self.senders {
            let cloned_senders = self.senders.clone();
            s1.send(TestMessage::AddSenders(cloned_senders)).unwrap();
            // chaos monkey ignores the count
            s1.send(TestMessage::Notify(notifier.clone(), 0)).unwrap();
        }
        let (sender, receiver) = channel();
        notifier.send(TestMessage::Notify(sender, self.message_count)).unwrap();
        self.receiver = Some(receiver);
        log::debug!("chaos_monkey: monkeys wired");

        // wait for the scheduler/executor to get them all assigned
        if wait_for_machine_setup(self.baseline + self.machine_count - 1).is_err() {
            panic!("chaos_monkey: machine setup failed");
        }
        log::debug!("chaos_monkey: setup complete");
    }

    // tear down the machines
    fn teardown(mut chaos_monkey: Self) {
        log::debug!("chaos_monkey: tear-down started");
        let baseline = chaos_monkey.baseline;
        // due to a sender pointing to its own receiver, we need to dismantle senders.
        chaos_monkey
            .senders
            .drain(..)
            .for_each(|s| s.send(TestMessage::RemoveAllSenders).unwrap());
        chaos_monkey.receiver = None;
        drop(chaos_monkey);

        // wait for the machines to all go away
        if wait_for_machine_teardown(baseline).is_err() {
            panic!("chaos_monkey: machine tear-down failed");
        }
        log::debug!("chaos_monkey: tear-down complete");
    }

    // run a single iteration
    fn run(&self) {
        let range = Uniform::from(0 .. self.senders.len());
        let mut rng = rand::rngs::OsRng::default();
        for _ in 0 .. self.message_count {
            let idx = range.sample(&mut rng);
            self.senders[idx]
                .send(TestMessage::ChaosMonkey {
                    counter: 0,
                    max: self.inflection_value,
                    mutation: ChaosMonkeyMutation::Increment,
                })
                .unwrap();
        }
        if let Some(receiver) = self.receiver.as_ref() {
            if wait_for_notification(receiver, self.message_count, self.duration).is_err() {
                panic!("chaos_monkey: completion notification failed");
            }
        }
    }
}
