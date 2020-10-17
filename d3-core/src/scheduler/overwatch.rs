use self::traits::*;
use super::*;

type MonitorReceiver = crossbeam_channel::Receiver<MonitorMessage>;

// The factory for creating the system monitor.
pub struct SystemMonitorFactory {
    sender: MonitorSender,
    receiver: MonitorReceiver,
}
impl SystemMonitorFactory {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> MonitorFactoryObj {
        let (sender, receiver) = crossbeam_channel::bounded::<MonitorMessage>(MONITOR_QUEUE_MAX);
        Arc::new(Self { sender, receiver })
    }
}

impl MonitorFactory for SystemMonitorFactory {
    fn get_sender(&self) -> MonitorSender { self.sender.clone() }
    fn start(&self, executor: ExecutorControlObj) -> MonitorControlObj {
        SystemMonitor::start(self.sender.clone(), self.receiver.clone(), executor)
    }
}

const MONITOR_QUEUE_MAX: usize = 100;

// There should only be one system monitor
pub struct SystemMonitor {
    sender: MonitorSender,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SystemMonitor {
    // Start the system monitor. The sooner, the better.
    fn start(sender: MonitorSender, receiver: MonitorReceiver, executor: ExecutorControlObj) -> MonitorControlObj {
        let monitor = Self {
            sender,
            thread: ThreadData::spawn(receiver, executor),
        };
        Arc::new(monitor)
    }

    // Stop the system monitor. Late stopping is recommended.
    fn stop(&self) { if self.sender.send(MonitorMessage::Terminate).is_err() {} }
}

impl MonitorControl for SystemMonitor {
    // stop the system monitor
    fn stop(&self) { self.stop(); }
    // add a stats sender to the system monitor
    fn add_sender(&self, sender: CoreStatsSender) { if self.sender.send(MonitorMessage::AddSender(sender)).is_err() {} }
    // remove a stats sender to the system monitor
    fn remove_sender(&self, sender: CoreStatsSender) {
        if self.sender.send(MonitorMessage::AddSender(sender)).is_err() {}
    }
}

// If we haven't done so already, attempt to stop the system monitor thread
impl Drop for SystemMonitor {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            if self.sender.send(MonitorMessage::Terminate).is_err() {}
            log::info!("synchronizing system monitor shutdown");
            if thread.join().is_err() {
                log::trace!("failed to join system monitor thread");
            }
            log::info!("System Monitor shut down");
        }
    }
}

// The monitor runs in a thread, this is its data
struct ThreadData {
    receiver: MonitorReceiver,
    executor: ExecutorControlObj,
    senders: Vec<CoreStatsSender>,
}
impl ThreadData {
    // launch the long running system monitor thread
    fn spawn(receiver: MonitorReceiver, executor: ExecutorControlObj) -> Option<std::thread::JoinHandle<()>> {
        let thread = thread::spawn(move || {
            let mut res = Self {
                receiver,
                executor,
                senders: Vec::new(),
            };
            res.run()
        });
        Some(thread)
    }

    // the system monitor run loop. It doesn't do much.
    fn run(&mut self) {
        log::info!("System Monitor is running");
        loop {
            match self.receiver.recv() {
                Err(_e) => break,
                Ok(m) => match m {
                    MonitorMessage::Terminate => break,
                    MonitorMessage::Parked(id) => {
                        log::info!("System Monitor: Executor {} is parked", id);
                        self.executor.parked_executor(id);
                    },
                    MonitorMessage::Terminated(id) => self.executor.joinable_executor(id),
                    MonitorMessage::AddSender(sender) => self.add_sender(sender),
                    MonitorMessage::RemoveSender(sender) => self.remove_sender(sender),
                    MonitorMessage::ExecutorStats(stats) => self.try_fwd(CoreStatsMessage::ExecutorStats(stats)),
                    MonitorMessage::SchedStats(stats) => self.try_fwd(CoreStatsMessage::SchedStats(stats)),

                    #[allow(unreachable_patterns)]
                    _ => log::info!("System Monitor recevied an unhandled message {:#?}", m),
                },
            }
        }
        log::info!("System Monitor is stopped");
    }

    // add sender if not already in the list
    fn add_sender(&mut self, sender: CoreStatsSender) {
        for s in &self.senders {
            if sender.sender.same_channel(&s.sender) {
                return;
            }
        }
        self.senders.push(sender);
    }

    // remove sender if in the list (drain_filter() is still experimental)
    fn remove_sender(&mut self, sender: CoreStatsSender) {
        let mut index = usize::MAX;
        for (idx, s) in self.senders.iter().enumerate() {
            if sender.sender.same_channel(&s.sender) {
                index = idx;
                break;
            }
        }
        if index != usize::MAX {
            self.senders.swap_remove(index);
        }
    }

    fn try_fwd(&mut self, msg: CoreStatsMessage) {
        use crossbeam_channel::TrySendError;
        match self.senders.len() {
            0 => (),
            1 => {
                if let Err(e) = self.senders[0].try_send(msg) {
                    if let TrySendError::<_>::Disconnected(_) = e {
                        self.senders.clear()
                    }
                }
            },
            _ => {
                let mut alive: Vec<CoreStatsSender> = Vec::new();
                for s in self.senders.drain(..) {
                    match s.try_send(msg) {
                        Ok(()) => alive.push(s),
                        Err(TrySendError::<_>::Full(_)) => alive.push(s),
                        _ => (),
                    }
                }
                self.senders = alive;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    struct Dummy {}
    impl ExecutorControl for Dummy {
        // notifies the executor that an executor is parked
        fn parked_executor(&self, _id: usize) {}
        // Wake parked threads
        fn wake_parked_threads(&self) {}
        // notifies the executor that an executor completed and can be joined
        fn joinable_executor(&self, _id: usize) {}
        // stop the executor
        fn stop(&self) {}
    }

    #[test]
    fn can_terminate() {
        let factory = SystemMonitorFactory::new();
        let executor: ExecutorControlObj = Arc::new(Dummy {});
        let monitor = factory.start(executor);
        thread::sleep(Duration::from_millis(100));
        monitor.stop();
        thread::sleep(Duration::from_millis(100));
    }
}
