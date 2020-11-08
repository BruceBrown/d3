use self::traits::*;
use super::*;

use crossbeam::channel::RecvTimeoutError;

type MachineMap = super_slab::SuperSlab<ShareableMachine>;

// The scheduler is responsible for the life-cycle of a machine.
//
// It starts with a machine being built and it being assigned.
// When it receives messages, the machine is given to the executor
// as a task, by the sender. If the machine SendBlocks the
// executor send it back to the scheduler to be re-scheduled.
// when disconnected, the executor sends it back to the sched
// to be destroyed.
//
// Some thing of note:
// * Crossbeam Deque is the task queue
// * SuperSlab is used as a container of machines.
//

// Tuning for the scheduler, the count if for slab and map index sizing.
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
/// The machine_count_estimate is an estimate for the number of machines
/// that will exist at any point in time. A SuperSlab is used for tracking
/// machines and mis-estimating will cause allocation.
/// The default is 5000 machines.
pub static machine_count_estimate: AtomicCell<usize> = AtomicCell::new(5000);

/// The get_machine_count_estimate function returns the current estimate.
#[allow(dead_code)]
pub fn get_machine_count_estimate() -> usize { machine_count_estimate.load() }

/// The set_machine_count_estimate function sets the current estimate. The
/// estimate should be set before starting the server.
#[allow(dead_code)]
pub fn set_machine_count_estimate(new: usize) { machine_count_estimate.store(new); }

/// The selector_maintenance_duration determines how often the selector will yield
/// for maintanance. It will also yeild when it has accumulated enough debt to warrant yielding.
#[allow(dead_code, non_upper_case_globals)]
#[deprecated(since = "0.1.2", note = "select is no longer used by the scheduler")]
pub static selector_maintenance_duration: AtomicCell<Duration> = AtomicCell::new(Duration::from_millis(500));

/// The get_selector_maintenance_duration function returns the current maintenance duration.
#[allow(dead_code, non_upper_case_globals, deprecated)]
#[deprecated(since = "0.1.2", note = "select is no longer used by the scheduler")]
pub fn get_selector_maintenance_duration() -> Duration { selector_maintenance_duration.load() }

/// The set_selector_maintenance_duration function sets the current maintenance duration.
#[allow(dead_code, non_upper_case_globals, deprecated)]
#[deprecated(since = "0.1.2", note = "select is no longer used by the scheduler")]
pub fn set_selector_maintenance_duration(new: Duration) { selector_maintenance_duration.store(new); }

/// The live_machine_count is the number of machines in the collective.
#[allow(dead_code, non_upper_case_globals)]
pub static live_machine_count: AtomicUsize = AtomicUsize::new(0);

/// The get_machine_count function returns the number of machines in the collective.
#[allow(dead_code, non_upper_case_globals)]
pub fn get_machine_count() -> usize { live_machine_count.load(Ordering::SeqCst) }

/// Statistics for the schdeduler
#[derive(Debug, Default, Copy, Clone)]
pub struct SchedStats {
    pub maint_time: Duration,
    pub add_time: Duration,
    pub remove_time: Duration,
    pub total_time: Duration,
}

// The default scheduler. It is created by the scheduler factory.
#[allow(dead_code)]
pub struct DefaultScheduler {
    sender: SchedSender,
    wait_queue: SchedTaskInjector,
    thread: Option<thread::JoinHandle<()>>,
}
impl DefaultScheduler {
    // stop the scheduler
    fn stop(&self) {
        log::info!("stopping scheduler");
        self.sender.send(SchedCmd::Stop).unwrap();
    }
    // create the scheduler
    pub fn new(sender: SchedSender, receiver: SchedReceiver, monitor: MonitorSender, queues: (TaskInjector, SchedTaskInjector)) -> Self {
        let wait_queue = Arc::clone(&queues.1);
        let thread = SchedulerThread::spawn(receiver, monitor, queues);
        sender.send(SchedCmd::Start).unwrap();
        Self {
            wait_queue,
            sender,
            thread,
        }
    }
}

impl Scheduler for DefaultScheduler {
    // assign a new machine into the collective
    fn assign_machine(&self, machine: ShareableMachine) { self.sender.send(SchedCmd::New(machine)).unwrap(); }
    // request stats from the executors
    fn request_stats(&self) { self.sender.send(SchedCmd::RequestStats).unwrap(); }
    // request machine info
    fn request_machine_info(&self) { self.sender.send(SchedCmd::RequestMachineInfo).unwrap(); }
    // stop the scheduler
    fn stop(&self) { self.stop(); }
}

// If we haven't done so already, attempt to stop the schduler thread
impl Drop for DefaultScheduler {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            if self.sender.send(SchedCmd::Terminate(false)).is_err() {}
            log::info!("synchronizing Scheduler shutdown");
            if thread.join().is_err() {
                log::trace!("failed to join Scheduler thread");
            }
        }
        log::info!("Scheduler shutdown complete");
    }
}

// The schduler thread. Working through the borrow-checker made
// this an interesting design. At the top we have maintenance
// of the collective, where machines are inserted or removed.
// From there a select list is created for every machine ready
// to receive a command. That layer is is responsible for deciding
// which commands it can immediately handle and which need to
// be handled by the outer layer. Then we come to the final
// layer of the scheduler. Where it mantians a seconday select
// list for machines returing from the executor.
//
const MAX_SELECT_HANDLES: usize = usize::MAX - 16;

#[allow(dead_code)]
struct SchedulerThread {
    receiver: SchedReceiver,
    monitor: MonitorSender,
    wait_queue: SchedTaskInjector,
    run_queue: TaskInjector,
    is_running: bool,
    is_started: bool,
    machines: MachineMap,
}
impl SchedulerThread {
    // start the scheduler thread and call run()
    fn spawn(receiver: SchedReceiver, monitor: MonitorSender, queues: (TaskInjector, SchedTaskInjector)) -> Option<thread::JoinHandle<()>> {
        log::info!("Starting scheduler");
        let thread = std::thread::spawn(move || {
            let mut sched_thread = Self {
                receiver,
                monitor,
                run_queue: queues.0,
                wait_queue: queues.1,
                is_running: true,
                is_started: false,
                machines: MachineMap::with_capacity(get_machine_count_estimate()),
            };
            sched_thread.run();
        });
        Some(thread)
    }

    // This is the top layer, where machines are added or removed.
    // it calls the build select layer.
    fn run(&mut self) {
        log::info!("running schdeuler");
        let mut stats_timer = SimpleEventTimer::default();
        let start = Instant::now();
        let mut stats = SchedStats::default();
        while self.is_running {
            // event check
            if stats_timer.check() && self.monitor.send(MonitorMessage::SchedStats(stats)).is_err() {
                log::debug!("failed to send sched stats to mointor");
            }
            // wait for something to do
            match self.receiver.recv_timeout(stats_timer.remaining()) {
                Ok(cmd) => self.maintenance(cmd, &mut stats),
                Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => self.is_running = false,
            }
        }
        stats.total_time = start.elapsed();
        log::info!("machines remaining: {}", self.machines.len());
        log::info!("{:#?}", stats);
        log::info!("completed running schdeuler");
    }

    fn maintenance(&mut self, cmd: SchedCmd, stats: &mut SchedStats) {
        let t = Instant::now();
        match cmd {
            SchedCmd::Start => (),
            SchedCmd::Stop => self.is_running = false,
            SchedCmd::Terminate(_key) => (),
            SchedCmd::New(machine) => self.insert_machine(machine, stats),
            SchedCmd::SendComplete(key) => self.schedule_sendblock_machine(key),
            SchedCmd::Remove(key) => self.remove_machine(key, stats),
            SchedCmd::RecvBlock(key) => self.schedule_recvblock_machine(key),
            SchedCmd::RequestStats => if self.monitor.send(MonitorMessage::SchedStats(*stats)).is_err() {},
            SchedCmd::RequestMachineInfo => self.send_machine_info(),
            _ => (),
        };
        stats.maint_time += t.elapsed();
    }
    // insert a machine into the machines map, this is where the machine.key is set
    fn insert_machine(&mut self, machine: ShareableMachine, stats: &mut SchedStats) {
        let t = Instant::now();
        let entry = self.machines.vacant_entry();
        log::trace!("inserted machine {} key={}", machine.get_id(), entry.key());
        machine.key.store(entry.key(), Ordering::SeqCst);
        entry.insert(Arc::clone(&machine));

        // Always run a task, on insertion so that the machine gets the one-time
        // connection notification. Otherwise, it would have to wait for a send.
        if let Err(state) = machine.compare_and_exchange_state(MachineState::New, MachineState::Ready) {
            log::error!("insert_machine: expected state New, found state {:#?}", state);
        }
        live_machine_count.fetch_add(1, Ordering::SeqCst);
        schedule_machine(&machine, &self.run_queue);
        stats.add_time += t.elapsed();
    }

    fn remove_machine(&mut self, key: usize, stats: &mut SchedStats) {
        let t = Instant::now();
        // Believe it or not, this remove is a huge performance hit to
        // the scheduler. It results a whole bunch of drops being run.
        if let Some(machine) = self.machines.get(key) {
            log::trace!("removed machine {} key={}", machine.get_id(), machine.get_key());
        } else {
            log::warn!("machine key {} not in collective", key);
            stats.remove_time += t.elapsed();
            return;
        }
        self.machines.remove(key);
        live_machine_count.fetch_sub(1, Ordering::SeqCst);
        stats.remove_time += t.elapsed();
    }

    fn send_machine_info(&self) {
        for (_, m) in &self.machines {
            if self.monitor.send(MonitorMessage::MachineInfo(Arc::clone(m))).is_err() {
                log::debug!("unable to send machine info to monitor");
            }
        }
    }
    fn run_task(&self, machine: &ShareableMachine) {
        if let Err(state) = machine.compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready) {
            if state != MachineState::Ready {
                log::error!("sched run_task expected RecvBlock or Ready state{:#?}", state);
            }
        }
        schedule_machine(machine, &self.run_queue);
    }

    fn schedule_sendblock_machine(&self, key: usize) {
        // log::trace!("sched SendComplete machine {}", key);
        let machine = self.machines.get(key).unwrap();

        if let Err(state) = machine.compare_and_exchange_state(MachineState::SendBlock, MachineState::RecvBlock) {
            log::error!("sched: (SendBlock) expecting state SendBlock, found {:#?}", state);
            return;
        }
        if !machine.is_channel_empty()
            && machine
                .compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready)
                .is_ok()
        {
            schedule_machine(machine, &self.run_queue);
        }
    }

    fn schedule_recvblock_machine(&self, key: usize) {
        // log::trace!("sched RecvBlock machine {}", key);
        let machine = self.machines.get(key).unwrap();
        if machine
            .compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready)
            .is_ok()
        {
            schedule_machine(machine, &self.run_queue);
        }
    }
}

#[cfg(test)]
mod tests {
    use self::executor::SystemExecutorFactory;
    use self::machine::get_default_channel_capacity;
    use self::overwatch::SystemMonitorFactory;
    use self::sched_factory::create_sched_factory;
    use super::*;
    use crossbeam::deque;
    use d3_derive::*;
    use std::time::Duration;

    use self::channel::{
        machine_channel::{channel, channel_with_capacity},
        receiver::Receiver,
        sender::Sender,
    };

    #[test]
    fn can_terminate() {
        let monitor_factory = SystemMonitorFactory::new();
        let executor_factory = SystemExecutorFactory::new();
        let scheduler_factory = create_sched_factory();

        let scheduler = scheduler_factory.start(monitor_factory.get_sender(), executor_factory.get_queues());
        thread::sleep(Duration::from_millis(100));
        log::info!("stopping scheduler via control");
        scheduler.stop();
        thread::sleep(Duration::from_millis(100));
    }

    #[derive(Debug, MachineImpl)]
    pub enum TestMessage {
        Test,
    }

    // A simple Alice machine
    struct Alice {}
    impl Machine<TestMessage> for Alice {
        fn receive(&self, _message: TestMessage) {}
    }

    #[allow(clippy::type_complexity)]
    pub fn build_machine<T, P>(
        machine: T,
    ) -> (
        Arc<Mutex<T>>,
        Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
        MachineAdapter,
    )
    where
        T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
        P: MachineImpl,
        <P as MachineImpl>::Adapter: MachineBuilder,
    {
        let channel_max = get_default_channel_capacity();
        let (machine, sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_raw(machine, channel_max);
        // let collective_adapter = Arc::new(Mutex::new(collective_adapter));
        // Server::assign_machine(collective_adapter);
        (machine, sender, collective_adapter)
    }

    #[test]
    fn test_scheduler() {
        let (monitor_sender, _monitor_receiver) = crossbeam::channel::unbounded::<MonitorMessage>();
        let (sched_sender, sched_receiver) = crossbeam::channel::unbounded::<SchedCmd>();
        let run_queue = Arc::new(deque::Injector::<Task>::new());
        let wait_queue = Arc::new(deque::Injector::<SchedTask>::new());

        let thread = SchedulerThread::spawn(sched_receiver, monitor_sender, (run_queue, wait_queue));
        // at this point the scheduler should be running
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut senders: Vec<Sender<TestMessage>> = Vec::new();
        let mut machines: Vec<Arc<Mutex<Alice>>> = Vec::new();
        // build 5 alice machines
        for _ in 1 ..= 5 {
            let alice = Alice {};
            let (alice, mut sender, adapter) = build_machine(alice);
            let adapter = Arc::new(adapter);
            sender.bind(Arc::downgrade(&adapter));
            senders.push(sender);
            machines.push(alice);
            sched_sender.send(SchedCmd::New(adapter)).unwrap();
        }

        let s = &senders[2];
        s.send(TestMessage::Test).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));

        sched_sender.send(SchedCmd::Stop).unwrap();
        if let Some(thread) = thread {
            thread.join().unwrap();
        }
    }
}
