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
}
impl FanoutFaninDriver {
    pub fn setup(&mut self) {
        self.baseline = executor::get_machine_count();
        let (_f, fanout_sender) = if self.bound_queue {
            executor::connect(Forwarder::new(1))
        } else {
            executor::connect_unbounded(Forwarder::new(1))
        };
        let (_f, fanin_sender) = if self.bound_queue {
            executor::connect(Forwarder::new(2))
        } else {
            executor::connect_unbounded(Forwarder::new(2))
        };
        for idx in 3 ..= self.machine_count {
            let (_f, s) = if self.bound_queue {
                executor::connect(Forwarder::new(idx))
            } else {
                executor::connect_unbounded(Forwarder::new(idx))
            };
            fanout_sender.send(TestMessage::AddSender(s.clone())).unwrap();
            s.send(TestMessage::AddSender(fanin_sender.clone())).unwrap();
            self.senders.push(s);
        }
        self.fanout_sender = Some(fanout_sender);
        // turn the fanin into a notifier
        let (sender, receiver) = channel();
        self.receiver = Some(receiver);
        let expect_count = (self.machine_count - 2) * self.message_count;
        fanin_sender.send(TestMessage::Notify(sender, expect_count)).unwrap();
        // wait for the scheduler/executor to get them all assigned
        loop {
            thread::yield_now();
            if executor::get_machine_count() >= self.baseline + self.machine_count - 1 {
                break;
            }
        }
    }
    pub fn teardown(fanout_fanin: Self) {
        let baseline = fanout_fanin.baseline;
        // drop, wiping out all senders/receivers/machines
        drop(fanout_fanin);
        // wait for the machines to all go away
        loop {
            thread::yield_now();
            if baseline == executor::get_machine_count() {
                break;
            }
        }
    }
    pub fn run(&self) {
        if let Some(sender) = self.fanout_sender.as_ref() {
            for _ in 0 .. self.message_count {
                sender.send(TestMessage::Test).unwrap();
            }
            if let Some(receiver) = self.receiver.as_ref() {
                let expect_count = (self.machine_count - 2) * self.message_count;
                wait_for_notification(&receiver, expect_count, self.duration);
            }
        }
    }
}
