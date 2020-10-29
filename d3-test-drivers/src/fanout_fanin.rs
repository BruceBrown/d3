use self::forwarder::Forwarder;
use super::*;

/// FanoutFanin will setup a network of machines where the first machine forwards
/// the received message to all of the intermediate macines. That's the fanout leg.
/// Each of the intermediate machines forwards the message it receives to single
/// machine, that's the fanin leg. When that machine receives all of the expected
/// messages it will send a notification, ending the test. This will stress the
/// executor as it may need to park senders due to a full message queue.
#[derive(Debug, SmartDefault)]
pub struct FanoutFaninDriver {
    #[default = 500]
    pub machine_count: usize,

    #[default = 15]
    pub message_count: usize,

    #[default = true]
    pub bound_queue: bool,

    #[default(Duration::from_secs(30))]
    pub duration: Duration,

    #[default(Vec::with_capacity(510))]
    senders: Vec<TestMessageSender>,

    fanout_sender: Option<TestMessageSender>,
    receiver: Option<TestMessageReceiver>,
    baseline: usize,
    #[default(AtomicUsize::new(1))]
    iteration: AtomicUsize,
}
impl FanoutFaninDriver {
    pub fn setup(&mut self) {
        self.baseline = executor::get_machine_count();
        let (_, fanout_sender) = if self.bound_queue {
            executor::connect(Forwarder::new(1))
        } else {
            executor::connect_unbounded(Forwarder::new(1))
        };
        let (_, fanin_sender) = if self.bound_queue {
            executor::connect(Forwarder::new(2))
        } else {
            executor::connect_unbounded(Forwarder::new(2))
        };
        for idx in 3 ..= self.machine_count {
            let (_, s) = if self.bound_queue {
                executor::connect(Forwarder::new(idx))
            } else {
                executor::connect_unbounded(Forwarder::new(idx))
            };
            fanout_sender.send(TestMessage::AddSender(s.clone())).unwrap();
            s.send(TestMessage::AddSender(fanin_sender.clone())).unwrap();
            self.senders.push(s);
        }
        log::debug!("fanout chan {} fanin {}", fanout_sender.get_id(), fanin_sender.get_id());
        self.fanout_sender = Some(fanout_sender);
        // turn the fanin into a notifier
        let (sender, receiver) = channel();
        self.receiver = Some(receiver);
        let expect_count = (self.machine_count - 2) * self.message_count;
        fanin_sender.send(TestMessage::Notify(sender, expect_count)).unwrap();

        // wait for the scheduler/executor to get them all assigned
        log::debug!("fanout_fanin: base-line: {}", self.baseline);
        loop {
            thread::yield_now();
            if executor::get_machine_count() >= self.baseline + self.machine_count - 1 {
                break;
            }
        }
        log::debug!("fanout_fanin: setup complete");
    }
    pub fn teardown(fanout_fanin: Self) {
        log::debug!("fanout_fanin: tear-down started");
        let baseline = fanout_fanin.baseline;
        for s in &fanout_fanin.senders {
            s.send(TestMessage::RemoveAllSenders).unwrap();
        }
        // drop, wiping out all senders/receivers/machines
        drop(fanout_fanin);
        let mut start = std::time::Instant::now();
        // wait for the machines to all go away
        loop {
            thread::yield_now();
            if baseline >= executor::get_machine_count() {
                break;
            }
            if start.elapsed() >= std::time::Duration::from_secs(1) {
                start = std::time::Instant::now();
                log::debug!("baseline {}, count {}", baseline, executor::get_machine_count());
            }
        }
        log::debug!("fanout_fanin: tear-down complete");
    }
    pub fn run(&self) {
        // let count = self.iteration.fetch_add(1, Ordering::SeqCst);
        // log::info!("fanout_fanin iteration: {}", count);
        if let Some(sender) = self.fanout_sender.as_ref() {
            for _ in 0 .. self.message_count {
                sender.send(TestMessage::Test).unwrap();
            }
            if let Some(receiver) = self.receiver.as_ref() {
                let expect_count = (self.machine_count - 2) * self.message_count;
                wait_for_notification(receiver, expect_count, self.duration);
            }
        }
    }
}
