use super::*;

type MonitorReceiver = crossbeam::Receiver<MonitorMessage>;

/// The factory for creating the system monitor.
pub struct SystemMonitorFactory {
    sender: MonitorSender,
    receiver: MonitorReceiver,
}
impl SystemMonitorFactory {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> MonitorFactoryObj {
        let (sender, receiver) = crossbeam::bounded::<MonitorMessage>(MONITOR_QUEUE_MAX);
        Arc::new(Self { sender, receiver })
    }
}

impl MonitorFactory for SystemMonitorFactory {
    fn get_sender(&self) -> MonitorSender {
        self.sender.clone()
    }
    fn start(&self, executor: ExecutorControlObj) -> MonitorControlObj {
        SystemMonitor::start(self.sender.clone(), self.receiver.clone(), executor)
    }
}

const MONITOR_QUEUE_MAX: usize = 100;

///
/// There should only be one system monitor
pub struct SystemMonitor {
    sender: MonitorSender,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SystemMonitor {
    /// Start the system monitor. The sooner, the better.
    fn start(
        sender: MonitorSender,
        receiver: MonitorReceiver,
        executor: ExecutorControlObj,
    ) -> MonitorControlObj {
        let monitor = Self {
            sender,
            thread: ThreadData::spawn(receiver, executor),
        };
        Arc::new(monitor)
    }

    /// Stop the system monitor. Late stopping is recommended.
    fn stop(&self) {
        if self.sender.send(MonitorMessage::Terminate).is_err(){}
    }
}

impl MonitorControl for SystemMonitor {
    /// stop the system monitor
    fn stop(&self) {
        self.stop();
    }
}

/// If we haven't done so already, attempt to stop the system monitor thread
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

/// The monitor runs in a thread, this is its data
struct ThreadData {
    receiver: MonitorReceiver,
    executor: ExecutorControlObj,
}
impl ThreadData {
    /// launch the long running system monitor thread
    fn spawn(
        receiver: MonitorReceiver,
        executor: ExecutorControlObj,
    ) -> Option<std::thread::JoinHandle<()>> {
        let thread = thread::spawn(move || {
            let mut res = Self { receiver, executor };
            res.run()
        });
        Some(thread)
    }

    /// the system monitor run loop. It doesn't do much.
    fn run(&mut self) {
        log::info!("System Monitor is running");
        loop {
            match self.receiver.recv() {
                Err(_e) => break,
                Ok(m) => match m {
                    MonitorMessage::ExecutorStats(_stats) => {  }
                    MonitorMessage::Terminate => break,
                    MonitorMessage::Parked(id) => {
                        log::info!("System Monitor: Executor {} is parked", id);
                        self.executor.parked_executor(id);
                    }
                    MonitorMessage::Terminated(id) => self.executor.joinable_executor(id),
                },
            }
        }
        log::info!("System Monitor is stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    struct dummy {}
    impl ExecutorControl for dummy {
        fn parked_executor(&self, id: u128) {
            panic!("executor should not be called")
        }
        fn stop(&self) {
            panic!("executor should not be called")
        }
    }

    #[test]
    fn can_terminate() {
        let factory = SystemMonitorFactory::new();
        let executor: ExecutorControlObj = Arc::new(dummy {});
        let monitor = factory.start(executor);
        thread::sleep(Duration::from_millis(100));
        monitor.stop();
        thread::sleep(Duration::from_millis(100));
    }
}
