use super::*;

///
/// These are traits that are used for major components
///

///
/// Messages which can be sent to the system monitor
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MonitorMessage {
    // Terminate the monitor
    Terminate,
    // Sent by an executor when it parks
    Parked(usize),
    // Sent by a executor as it completes, signalling that its joinable
    Terminated(usize),
    // Send by an executor, providing periodic stats
    ExecutorStats(ExecutorStats),
}
pub type MonitorSender = crossbeam::Sender<MonitorMessage>;

// The factory for the system monitor
pub trait MonitorFactory {
    /// get a clone of the sender for the system monitor
    fn get_sender(&self) -> MonitorSender;
    /// start the system monitor
    fn start(&self, executor: ExecutorControlObj) -> MonitorControlObj;
}
pub type MonitorFactoryObj = Arc<dyn MonitorFactory>;

// The controller for the system monitor.
pub trait MonitorControl: Send + Sync {
    /// stop the system monitor
    fn stop(&self);
}
pub type MonitorControlObj = Arc<dyn MonitorControl>;


/// The factory for the executor
pub trait ExecutorFactory {
    /// set the number of executor threads
    fn with_workers(&self, workers: usize);
    /// get the system queues: run_queue, wait_queue
    fn get_queues(&self) -> (TaskInjector, TaskInjector);
    /// start the executor
    fn start(&self, monitor: MonitorSender, scheduler: SchedSender) -> ExecutorControlObj;
}
pub type ExecutorFactoryObj = Arc<dyn ExecutorFactory>;

/// The model for a system queue
pub type TaskInjector = Arc<deque::Injector<Task>>;

pub trait ExecutorControl: Send + Sync {
    /// notifies the executor that an executor is parked
    fn parked_executor(&self, id: usize);
    /// notifies the executor that an executor completed and can be joined
    fn joinable_executor(&self, id: usize);
    /// stop the executor
    fn stop(&self);
}
pub type ExecutorControlObj = Arc<dyn ExecutorControl>;


/// The schdeuler and executor commands
#[allow(dead_code)]
pub enum SchedCmd {
    Start,
    Stop,
    Terminate(bool),
    New(ShareableMachine),
    Waiting(u128),
    Remove(u128),
    ErrorRecv(u128),
    RecvBlock(u128),
    // Executor stuff
    RebuildStealers,
}
pub type SchedSender = crossbeam::Sender<SchedCmd>;
pub type SchedReceiver = crossbeam::Receiver<SchedCmd>;

/// The factory for the scheduler
pub trait SchedulerFactory {
    /// get a clone of a sender for the scheduler
    fn get_sender(&self) -> SchedSender;
    /// start the scheduler
    fn start(
        &self,
        monitor: MonitorSender,
        queues: (TaskInjector, TaskInjector),
    ) -> SchedulerControlObj;
}
pub type SchedulerFactoryObj = Arc<dyn SchedulerFactory>;

/// The scheduler control
pub trait SchedulerControl: Send + Sync {
    /// assigns a new machine, making it eligable for scheduling and running
    fn assign_machine(&self, machine: SharedCollectiveAdapter);
    /// stop the scheduler
    fn stop(&self);
}
pub type SchedulerControlObj = Arc<dyn SchedulerControl>;

/// ShareableMachine is an alias which contends with SharedCollectiveAdapter
/// one of them needs to go...
pub type ShareableMachine = SharedCollectiveAdapter;
