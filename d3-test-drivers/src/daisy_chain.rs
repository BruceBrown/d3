
use super::*;
use self::forwarder::{Forwarder};

/// DaisyChain will setup a linear network of machines in which a messages
/// received by a machine is forwarded to the next. Every machine in the
/// network will be visited by the message. Send 400 message in a network
/// of 4000 machines produces 1,600,000 message propagations. Additionally,
/// the propagation through the network is essentially a single pulse wave.
/// In this case, a pulse of 400 messages.
#[derive(Debug, SmartDefault)]
pub struct DaisyChainDriver {
    #[default=4000]
    pub machine_count: usize,

    #[default=400]
    pub message_count: usize,

    #[default=true]
    pub bound_queue: bool,

    #[default=1]
    pub forwarding_multiplier: usize,

    #[default(Duration::from_secs(5))]
    pub duration: Duration,

    #[default(Vec::with_capacity(4010))]
    senders: Vec<TestMessageSender>,

    first_sender: Option<TestMessageSender>,
    receiver: Option<TestMessageReceiver>,
    baseline: usize,
    exepected_message_count: usize,
}
impl DaisyChainDriver {
    pub fn setup(&mut self) {
        self.baseline = executor::get_machine_count();
        let (_, s) = if self.bound_queue {
            executor::connect(Forwarder::new(1))
        } else {
            executor::connect_unbounded(Forwarder::new(1))
        };
        self.first_sender = Some(s.clone());
        let mut last_sender = s.clone();
        self.senders.push(s);

        for idx in 2..=self.machine_count {
            let (_, s) = if self.bound_queue {
                executor::connect(Forwarder::new(idx))
            } else {
                executor::connect_unbounded(Forwarder::new(idx))
            };
            last_sender.send(TestMessage::AddSender(s.clone())).unwrap();
            last_sender.send(TestMessage::ForwardingMultiplier(self.forwarding_multiplier)).unwrap();
            last_sender = s.clone();
            self.senders.push(s);
        }
        self.exepected_message_count =
            self.message_count * (self.forwarding_multiplier.pow((self.machine_count - 1) as u32));
        if self.forwarding_multiplier > 1 {
            log::info!("expecting {} messages", self.exepected_message_count);
        }
        // turn the last into a notifier
        let (sender, receiver) = channel();
        last_sender
            .send(TestMessage::Notify(sender, self.exepected_message_count))
            .unwrap();
        self.receiver = Some(receiver);

        // wait for the scheduler/executor to get them all assigned
        loop {
            thread::yield_now();
            if executor::get_machine_count() >= self.baseline + self.machine_count-1 { break }
        }
        log::info!("completed configuring machines for dasiy-chain test");
    }
    pub fn teardown(daisy_chain: Self) {
        let baseline = daisy_chain.baseline;
        // drop, wiping out all senders/receivers/machines
        drop(daisy_chain);
        // wait for the machines to all go away
        let mut count: usize = 0;
        loop {
            thread::yield_now();
            if baseline == executor::get_machine_count() { break }
            let remain = executor::get_machine_count();
            if remain != count {
                log::info!(" {} remain, baseline is {}", remain, baseline);
                count = remain;
            }
        }    
    }
    pub fn run(&self) {
        if let Some(sender) = self.first_sender.as_ref() {
            for msg_id in 0..self.message_count {
                sender.send(TestMessage::TestData(msg_id)).unwrap();
            }
            if let Some(receiver) = self.receiver.as_ref() {
                wait_for_notification(&receiver, self.exepected_message_count, self.duration);
            }
        }
    }
}