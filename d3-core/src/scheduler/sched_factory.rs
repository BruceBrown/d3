use self::sched::DefaultScheduler;
use self::traits::*;
use super::*;

/// The sceduler factory provides a standard interface for creating
/// a scheduler. Additionally, a different scheduler can be used dropped
/// in without impacting the interface.

/// create the factory for creating and starting the scheduler
pub fn create_sched_factory() -> impl SchedulerFactory { Factory::new() }

struct Factory {
    sender: SchedSender,
    receiver: SchedReceiver,
}
impl Factory {
    /// create the factory
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded::<SchedCmd>();
        Self { sender, receiver }
    }
    /// get the sender for the scheduler
    pub fn get_sender(&self) -> SchedSender { self.sender.clone() }
    /// start the scheduler
    pub fn create_and_start(&self, monitor: MonitorSender, queues: (ExecutorInjector, SchedTaskInjector)) -> SchedulerEnum {
        // this where different schedulers can be started
        log::info!("creating Scheduler");
        let s: SchedulerEnum = DefaultScheduler::new(self.sender.clone(), self.receiver.clone(), monitor, queues).into();
        s
    }
}

impl SchedulerFactory for Factory {
    fn get_sender(&self) -> SchedSender { self.sender.clone() }
    // start must return a sized object trait, I prefer Arc over Box
    fn start(&self, monitor: MonitorSender, queues: (ExecutorInjector, SchedTaskInjector)) -> SchedulerEnum {
        self.create_and_start(monitor, queues)
    }
}
