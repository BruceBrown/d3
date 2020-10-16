use self::traits::*;
use super::*;

use self::setup_teardown::Server;
use crossbeam::{ReadyTimeoutError, RecvError, TryRecvError};

type FnvIndexMap<K, V> = indexmap::IndexMap<K, V, fnv::FnvBuildHasher>;
type SelIndexMap = FnvIndexMap<usize, usize>;
type TimeStampedSelIndexMap = FnvIndexMap<usize, (Instant, usize)>;
type MachineMap = slab::Slab<ShareableMachine>;

// The scheduler is responsible for the life-cycle of a machine.
//
// It starts with a machine being built and it being assigned.
// When it receives messages, the machine is given to the executor
// as a task. It then returns back to the scheduler to await
// further instructions, or destroeyed if its channel has closed.
//
// Some thing of note:
// * Crossbeam Select signals receiver readiness
// * Crossbeam Deque is the task queue
// * IndexMap is used for translating select index into machine key
// * Fnv is the hasher used for IndexMap
// * Slab is used as a container of machines.
//

// Tuning for the scheduler, the count if for slab and map index sizing.
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
/// The machine_count_estimate is an estimate for the number of machines
/// that will exist at any point in time. A Slab is used for tracking
/// machines and mis-estimating will cause reallocation and data movement.
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
pub static selector_maintenance_duration: AtomicCell<Duration> = AtomicCell::new(Duration::from_millis(500));

/// The get_selector_maintenance_duration function returns the current maintenance duration.
#[allow(dead_code, non_upper_case_globals)]
pub fn get_selector_maintenance_duration() -> Duration { selector_maintenance_duration.load() }

/// The get_selector_maintenance_duration function returns the current maintenance duration.
#[allow(dead_code, non_upper_case_globals)]
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
    pub new_time: Duration,
    pub rebuild_time: Duration,
    pub time_on_queue: Duration,
    pub resched_time: Duration,
    pub select_time: Duration,
    pub total_time: Duration,
    pub empty_select: u64,
    pub selected_count: u64,
    pub primary_select_count: u64,
    pub slow_select_count: u64,
    pub fast_select_count: u64,
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
    pub fn new(
        sender: SchedSender,
        receiver: SchedReceiver,
        monitor: MonitorSender,
        queues: (TaskInjector, SchedTaskInjector),
    ) -> Self {
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
    fn assign_machine(&self, machine: MachineAdapter) { self.sender.send(SchedCmd::New(machine)).unwrap(); }
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
    fn spawn(
        receiver: SchedReceiver,
        monitor: MonitorSender,
        queues: (TaskInjector, SchedTaskInjector),
    ) -> Option<thread::JoinHandle<()>> {
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
        let h = fnv::FnvBuildHasher::default();
        let mut recv_map = SelIndexMap::with_capacity_and_hasher(get_machine_count_estimate(), h);
        while self.is_running {
            // wait for some maintenance, which build_select supplies
            let results = self.build_select(&mut recv_map, &mut stats_timer, &mut stats);
            let maint_start = Instant::now();
            for result in results {
                self.maintenance_result(result, &mut stats);
            }
            stats.maint_time += maint_start.elapsed();
        }
        stats.total_time = start.elapsed();
        log::info!("machines remaining: {}", self.machines.len());
        log::info!("{:#?}", stats);
        log::info!("completed running schdeuler");
    }

    // maintain the select list, rebuilding when necessary. Most other things
    // are passed back as results to be processed
    fn build_select(
        &mut self,
        recv_map: &mut SelIndexMap,
        stats_timer: &mut SimpleEventTimer,
        stats: &mut SchedStats,
    ) -> Vec<Result<SchedCmd, crossbeam::RecvError>> {
        let mut select = self.build_select_from_ready(recv_map, stats);
        let mut results: Vec<Result<SchedCmd, crossbeam::RecvError>> = Vec::with_capacity(20);
        // results contains dead machines, unfotunately, this layer can't remove them
        let mut running = self.is_running;
        // last index is used to monitor if we're running out of handles in the select
        let mut last_index: usize = 1;
        while running && last_index < MAX_SELECT_HANDLES {
            let select_results = self.selector(&mut select, recv_map, stats_timer, stats);
            for result in select_results {
                match result {
                    Err(e) => {
                        results.push(Err(e));
                        running = false;
                    },
                    Ok(SchedCmd::Start) => (),
                    Ok(SchedCmd::Stop) => {
                        results.push(Ok(SchedCmd::Stop));
                        running = false;
                    },
                    Ok(SchedCmd::New(machine)) => {
                        results.push(Ok(SchedCmd::New(machine)));
                        running = false;
                    },
                    Ok(SchedCmd::Remove(id)) => {
                        results.push(Ok(SchedCmd::Remove(id)));
                        running = false;
                    },
                    Ok(SchedCmd::RecvBlock(id, exec_start)) => {
                        stats.time_on_queue += exec_start.elapsed();
                        // just extend the select list and update the recv_map for the index
                        let t = Instant::now();
                        let machine = self.machines.get(id).unwrap();
                        machine.state.set(CollectiveState::RecvBlock);
                        if last_index < MAX_SELECT_HANDLES {
                            last_index = machine.sel_recv(&mut select);
                            recv_map.insert(last_index, machine.key);
                        } else {
                            running = false;
                        }
                        stats.resched_time += t.elapsed();
                    },
                    Ok(_) => {
                        log::info!("scheduer builder received an unhandled cmd");
                    },
                }
            }
            if !running {
                log::debug!("build_select is returning");
            }
        }
        results
    }

    // loop running select. If commands are received, return them as a result.
    // Otherwise, receive an instruction and crate a task for the machine to
    // receive it.
    fn selector(
        &self,
        select: &mut crossbeam::Select,
        recv_map: &mut SelIndexMap,
        stats_timer: &mut SimpleEventTimer,
        stats: &mut SchedStats,
    ) -> Vec<Result<SchedCmd, RecvError>> {
        log::debug!("selector recv_map has {} entries", recv_map.len());
        let mut results = SchedResults::new();
        let h = fnv::FnvBuildHasher::default();
        let mut fast_recv_map = TimeStampedSelIndexMap::with_capacity_and_hasher(get_machine_count_estimate(), h);
        let mut fast_select = crossbeam::Select::new();
        fast_select.recv(&self.receiver);
        let mut last_index = 0;
        let worker = crossbeam::deque::Worker::<SchedTask>::new_fifo();
        loop {
            if stats_timer.check() && self.monitor.send(MonitorMessage::SchedStats(*stats)).is_err() {}
            // accumulate results, but don't hold them for too long
            if results.should_publish() {
                break;
            }
            let start_match = Instant::now();
            // get machines from the wait queue and setup select for each one
            let _ = self.wait_queue.steal_batch(&worker);
            while let Some(task) = worker.pop() {
                if last_index < MAX_SELECT_HANDLES {
                    let machine = self.machines.get(task.machine_key).unwrap();
                    machine.state.set(CollectiveState::RecvBlock);
                    last_index = machine.sel_recv(&mut fast_select);
                    fast_recv_map.insert(last_index, (Instant::now(), task.machine_key));
                } else {
                    results.push(Ok(SchedCmd::RecvBlock(task.machine_key, Instant::now())))
                }
            }
            // see if any machine's receiver is ready
            match Self::do_select(&mut fast_select, select, results.timeout()) {
                Err(ReadyTimeoutError) => {
                    stats.empty_select += 1;
                    self.maybe_wake_executor(None);
                },
                Ok((is_fast_select, index)) => {
                    stats.selected_count += 1;
                    if index == 0 {
                        stats.primary_select_count += 1;
                        match self.receiver.try_recv() {
                            Ok(cmd) => results.push(Ok(cmd)),
                            Err(TryRecvError::Disconnected) => results.push(Err(RecvError)),
                            Err(TryRecvError::Empty) => (),
                        }
                    } else if is_fast_select {
                        stats.fast_select_count += 1;
                        if let Some((_timestamp, key)) = fast_recv_map.get(&index) {
                            if let Some(machine) = self.machines.get(*key) {
                                match machine.try_recv_task(machine) {
                                    None => (),
                                    Some(task) => self.run_task(task),
                                }
                            }
                            fast_select.remove(index);
                            fast_recv_map.remove(&index);
                        } else {
                            log::error!("recv_map missing value for key {}", index);
                        }
                    } else {
                        stats.slow_select_count += 1;
                        if let Some(id) = recv_map.get(&index) {
                            if let Some(machine) = self.machines.get(*id) {
                                match machine.try_recv_task(machine) {
                                    None => (),
                                    Some(task) => self.run_task(task),
                                }
                            }
                            select.remove(index);
                            recv_map.remove(&index);
                        } else {
                            log::error!("recv_map missing value for key {}", index);
                        }
                    }
                },
            }
            stats.select_time = start_match.elapsed();
        }
        for (_, v) in fast_recv_map {
            results.push(Ok(SchedCmd::RecvBlock(v.1, v.0)));
        }
        let res = results.unwrap();
        log::debug!("selector returning with {} results", res.len());
        res
    }

    // insert a machine into the machines map, this is where the machine.key is set
    fn insert_machine(&mut self, mut machine: MachineAdapter, stats: &mut SchedStats) {
        let t = Instant::now();
        machine.state.set(CollectiveState::RecvBlock);
        let entry = self.machines.vacant_entry();
        machine.key = entry.key();
        log::trace!("inserted machine {} key={}", machine.get_id(), machine.key);
        entry.insert(Arc::new(machine));
        stats.new_time += t.elapsed();
        live_machine_count.fetch_add(1, Ordering::SeqCst);
    }
    fn maybe_wake_executor(&self, count: Option<usize>) {
        let in_flight = if let Some(count) = count {
            count
        } else {
            self::executor::RUN_QUEUE_LEN.load(Ordering::SeqCst)
        };
        if in_flight > 0 {
            let asleep = self::executor::EXECUTORS_SNOOZING.load(Ordering::SeqCst);
            if asleep > 0 {
                // log::debug!("in flight {}, waking {}", in_flight, asleep);
                Server::wake_executor_threads();
            }
        }
    }
    fn run_task(&self, task: Task) {
        // it is quite possible that the executors have all gone to sleep.
        // while we can ask deque if its empty, that's not quite good enough.
        // Instead, we check if there are executors awake, and if not we'll
        // wake them.
        let count = self::executor::RUN_QUEUE_LEN.fetch_add(1, Ordering::SeqCst);
        self.run_queue.push(task);
        self.maybe_wake_executor(Some(count + 1));
    }
    // create a select list from machines that are ready to receive. As much
    // as we'd like to clean up dead machines, that would create a data race
    // between the scheduler and executor.
    fn build_select_from_ready(&self, recv_map: &mut SelIndexMap, stats: &mut SchedStats) -> crossbeam::Select {
        let t = Instant::now();
        let mut sel = crossbeam::Select::new();
        // the first sel index is always our receiver
        sel.recv(&self.receiver);
        recv_map.clear();

        for (_, machine) in self.machines.iter() {
            if machine.get_state() == CollectiveState::RecvBlock {
                let idx = machine.sel_recv(&mut sel);
                recv_map.insert(idx, machine.key);
            }
        }
        stats.rebuild_time += t.elapsed();
        sel
    }

    // process results of a recv on the primary receiver
    fn maintenance_result(&mut self, result: Result<SchedCmd, RecvError>, stats: &mut SchedStats) {
        match result {
            Err(_e) => self.is_running = false,
            Ok(SchedCmd::Stop) => self.is_running = false,
            Ok(SchedCmd::New(machine)) => self.insert_machine(machine, stats),
            Ok(SchedCmd::Remove(id)) => {
                // Believe it or not, this remove is a huge performance hit to
                // the scheduler. It results a whole bunch of drops being run.
                log::trace!("removed machine {}", id);
                if let Some(machine) = self.machines.get(id) {
                    self.run_task(Task::new(machine));
                }
                self.machines.remove(id);
            },
            Ok(_) => log::warn!("scheduler cmd unhandled"),
        }
    }

    // get a ready index from 1 of 2 select lists
    fn do_select(
        fast: &mut crossbeam::Select,
        slow: &mut crossbeam::Select,
        duration: Duration,
    ) -> Result<(bool, usize), ReadyTimeoutError> {
        let start = Instant::now();
        let timeout = duration / 4;
        loop {
            match fast.try_ready() {
                Ok(index) => break Ok((true, index)),
                _ => match slow.ready_timeout(timeout) {
                    Ok(index) => break Ok((false, index)),
                    Err(e) => {
                        if start.elapsed() >= duration {
                            break Err(e);
                        }
                    },
                },
            }
        }
    }
}

// Encapsulation of results and the schedule for delivering them. This
// is just a little helper class to keep things neater in the selector.
struct SchedResults {
    results: Vec<Result<SchedCmd, RecvError>>,
    epoch: Instant,
    ready: usize,
    duration: Duration,
}
impl SchedResults {
    fn new() -> Self {
        Self {
            results: Vec::with_capacity(1000),
            epoch: Instant::now(),
            ready: 0,
            duration: get_selector_maintenance_duration(),
        }
    }

    // push results onto stack, note time if its the first
    fn push(&mut self, result: Result<SchedCmd, RecvError>) {
        if let Ok(SchedCmd::RecvBlock(_, _)) = result {
            self.ready += 1
        }
        if self.results.is_empty() {
            self.epoch = Instant::now()
        }
        self.results.push(result);
    }

    // publish if there are results, and they've aged long enough
    fn should_publish(&mut self) -> bool {
        if self.ready > 0 && self.epoch.elapsed() > Duration::from_millis(50) {
            true
        } else {
            !self.results.is_empty() && (self.epoch.elapsed() > self.duration || self.results.len() >= 1000)
        }
    }

    // compute a timeout that coincides with should_publish time
    fn timeout(&self) -> Duration {
        if self.ready == 0 {
            Duration::from_millis(20)
        } else {
            let e = self.epoch.elapsed();
            if e >= Duration::from_millis(20) {
                Duration::from_millis(1)
            } else {
                Duration::from_millis(20) - e
            }
        }
    }

    // unwrap the object, returning the accumulated results
    // clippy misunderstands and belives this can be a const fn
    #[allow(clippy::missing_const_for_fn)]
    fn unwrap(self) -> Vec<Result<SchedCmd, RecvError>> { self.results }
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
        let (machine, sender, collective_adapter) =
            <<P as MachineImpl>::Adapter as MachineBuilder>::build_raw(machine, channel_max);
        // let collective_adapter = Arc::new(Mutex::new(collective_adapter));
        // Server::assign_machine(collective_adapter);
        (machine, sender, collective_adapter)
    }

    #[test]
    fn test_scheduler() {
        // tweaks for more responsive testing
        set_selector_maintenance_duration(std::time::Duration::from_millis(20));

        let (monitor_sender, _monitor_receiver) = crossbeam::unbounded::<MonitorMessage>();
        let (sched_sender, sched_receiver) = crossbeam::unbounded::<SchedCmd>();
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
            let (alice, sender, adapter) = build_machine(alice);
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
