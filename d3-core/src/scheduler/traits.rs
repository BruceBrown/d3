use super::*;
use crossbeam::deque;

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
    fn get_queues(&self) -> (TaskInjector, SchedTaskInjector);
    /// start the executor
    fn start(&self, monitor: MonitorSender, scheduler: SchedSender) -> ExecutorControlObj;
}
pub type ExecutorFactoryObj = Arc<dyn ExecutorFactory>;

/// The model for a system queue
pub type TaskInjector = Arc<deque::Injector<Task>>;
pub type SchedTaskInjector = Arc<deque::Injector<SchedTask>>;

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
    New(MachineAdapter),
    Waiting(usize),
    Remove(usize),
    ErrorRecv(usize),
    RecvBlock(usize, Instant),
    // Executor stuff
    RebuildStealers,
}
pub type SchedSender = crossbeam::Sender<SchedCmd>;
pub type SchedReceiver = crossbeam::Receiver<SchedCmd>;

pub trait SchedulerFactory {
    fn get_sender(&self) -> SchedSender;
    // start has to return a sized object trait, I prefer Arc over Box
    fn start(&self, monitor: MonitorSender, queues: (TaskInjector, SchedTaskInjector)) -> Arc<dyn Scheduler>;
}
/// The scheduler trait
pub trait Scheduler: Send + Sync {
    /// assigns a new machine, making it eligable for scheduling and running
    fn assign_machine(&self, machine: MachineAdapter);
    /// stop the scheduler
    fn stop(&self);
}

