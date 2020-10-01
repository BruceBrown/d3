use super::*;
use crossbeam::{TryRecvError, RecvError, ReadyTimeoutError};

// The scheduler is responsible for accepting machines and waiting
// for them to receive instructions or for them to complete sending
// instructions along

// Essentially, there are any numer of machines, which are blocked
// on Recv or Send. Select is used to move them along to the executor.
// That's the drain side. The fill side is either fed from the executor,
// either finding a Recv block, Send block, or just too much time being
// used. So, it sends the machine back to the schdeuler. The other way
// the scheduler gets filled, is that new machines are created.
//

//
// The scheduler can take advantage of a local modification to select.
// the mod returns a vector of ready operations. This is controlled
// by #[cfg(select_multiple)]
//

//
// The factory as, well as the scheduler, are private. Traits are used
// to express the scheduler. I'm not 100% convinced that this is the right
// way to go. It allows for swapping out the scheduler. However, that
// could be done at the factory level too.
//

pub fn new_scheduler_factory() -> SchedulerFactoryObj {
    let (sender, receiver) = crossbeam::unbounded::<SchedCmd>();
    Arc::new(SystemSchedulerFactory { sender, receiver })
}

/// The scheduler factory, it is exposed as a trait object
struct SystemSchedulerFactory {
    sender: SchedSender,
    receiver: SchedReceiver,
}

/// The implementation of the trait object for the factory
impl SchedulerFactory for SystemSchedulerFactory {
    /// get a clone of the sender for the schdeduler
    fn get_sender(&self) -> SchedSender {
        self.sender.clone()
    }
    /// start the scheduler
    fn start(
        &self,
        monitor: MonitorSender,
        queues: (TaskInjector, TaskInjector),
    ) -> SchedulerControlObj {
        log::info!("creating Scheduler");
        let res = Scheduler::new(self.sender.clone(), self.receiver.clone(), monitor, queues);
        Arc::new(res)
    }
}

/// Statistics for the schdeduler
#[derive(Debug, Default, Copy, Clone)]
struct SelectStats {
    pub new_time: Duration,
    pub rebuild_time: Duration,
    pub resched_time: Duration,
    pub select_time: Duration,
    pub total_time: Duration,
    pub empty_select: u64,
    pub selected_count: u64,
}

/// The scheduler
#[allow(dead_code)]
struct Scheduler {
    sender: SchedSender,
    wait_queue: TaskInjector,
    thread: Option<thread::JoinHandle<()>>,
}
impl Scheduler {
    /// stop the scheduler
    fn stop(&self) {
        log::info!("stopping scheduler");
        self.sender.send(SchedCmd::Stop).unwrap();
    }
    /// create the scheduler
    fn new(
        sender: SchedSender,
        receiver: SchedReceiver,
        monitor: MonitorSender,
        queues: (TaskInjector, TaskInjector),
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


impl SchedulerControl for Scheduler {
    /// assign a new machine into the collective
    fn assign_machine(&self, machine: ShareableMachine) {
        self.sender.send(SchedCmd::New(machine)).unwrap();
    }
    /// stop the scheduler
    fn stop(&self) {
        self.stop();
    }
}

/// If we haven't done so already, attempt to stop the schduler thread
impl Drop for Scheduler {
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

/// The schduler thread. There are three levels within the scheduler. Some cause
/// a rebuild of the select list, some do not. At the top-level, there is scheduler
/// and machine maintanance. This can result in additions and removal of machines
/// and will rebuild the select list. The next level maintains the select list,
/// along with an parallel map for mapping a select index into a machine Id. Each
/// time it is entered, it creates a new select list, it then calls return. Upon
/// return, it attempts to extend the select list with machines which are waiting
/// for a recv to complete. Finally, we come to the selector, while there are
/// machines waiting to recv, and while there are not too many machines waiting
/// to be added to the select list, select is performed.
///
const MAX_SELECT_HANDLES: usize = usize::MAX - 16;

#[allow(dead_code)]
struct SchedulerThread {
    receiver: SchedReceiver,
    monitor: MonitorSender,
    wait_queue: TaskInjector,
    run_queue: TaskInjector,
    is_running: bool,
    is_started: bool,
    sel_generation: u64,
    machines: HashMap<u128, SharedCollectiveAdapter>,
}

impl SchedulerThread {
    /// start the scheduler thread and call run()
    fn spawn(
        receiver: SchedReceiver,
        monitor: MonitorSender,
        queues: (TaskInjector, TaskInjector),
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
                sel_generation: 0,
                machines: HashMap::with_capacity(5000),
            };
            sched_thread.run();
        });
        Some(thread)
    }

    // Get the schedulre running. At this point we can
    // add/remove machines, as we're going to rebuild everything.
    fn run(&mut self) {
        log::info!("running schdeuler");
        let start = Instant::now();
        let mut select_stats = SelectStats::default();
        while self.is_running {
            // wait for some maintenance, which build_select supplies
            let results = self.build_select(&mut select_stats);
            for result in results {
                match result {
                    Err(_e) => self.is_running = false,
                    Ok(SchedCmd::Stop) => self.is_running = false,
                    Ok(SchedCmd::New(machine)) => {
                        log::trace!("inserted machine {}", machine.id);
                        self.insert_machine(machine, &mut select_stats)
                    },
                    Ok(SchedCmd::Remove(id)) => {
                        log::trace!("removed machine {}", id);
                        self.machines.remove(&id);
                    },
                    Ok(_) => log::warn!("scheduler cmd unhandled"),
                }
            }
        }
        select_stats.total_time = start.elapsed();
        log::info!("machines remaining: {}", self.machines.len());
        log::info!("{:#?}", select_stats);
        log::info!("completed running schdeuler");
    }

    // maintain the select list, rebuilding when necessary. Most other things
    // are passed back as results to be processed
    fn build_select(
        &mut self,
        select_stats: &mut SelectStats,
    ) -> Vec<Result<SchedCmd, crossbeam::RecvError>> {
        let (mut select, mut recv_map, mut results) = self.build_select_from_ready(select_stats);
        // results contains dead machines, unfotunately, this layer can't remove them
        let mut running = self.is_running;
        // last index is used to monitor if we're running out of handles in the select
        let mut last_index: usize = 1;
        while running && last_index < MAX_SELECT_HANDLES {
            let select_results = self.selector(&mut select, &mut recv_map, select_stats);
            for result in select_results {
                match result {
                    Err(e) => {
                        results.push(Err(e));
                        running = false;
                    }
                    Ok(SchedCmd::Start) => (),
                    Ok(SchedCmd::Stop) => {
                        results.push(Ok(SchedCmd::Stop));
                        running = false;
                    }
                    Ok(SchedCmd::New(machine)) => {
                        results.push(Ok(SchedCmd::New(machine)));
                        running = false;
                    }
                    Ok(SchedCmd::Remove(id)) => {
                        results.push(Ok(SchedCmd::Remove(id)));
                        running = false;
                    }
                    Ok(SchedCmd::RecvBlock(id)) => {
                        // just extend the select list and update the recv_map for the index
                        let t = Instant::now();
                        let machine = self.machines.get(&id).unwrap();
                        machine.state.set(CollectiveState::RecvBlock);
                        if last_index < MAX_SELECT_HANDLES {
                            last_index = machine.sel_recv(&mut select);
                            recv_map.insert(last_index, machine.id);
                        } else {
                            running = false;
                        }
                        select_stats.resched_time += t.elapsed();
                    }
                    Ok(_) => {
                        log::info!("scheduer builder received an unhandled cmd");
                    }
                }
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
        recv_map: &mut HashMap<usize, u128>,
        stats: &mut SelectStats,
    ) -> Vec<Result<SchedCmd, RecvError>> {
        let mut results = SchedResults::new();
        loop {
            // accumulate results, but don't hold them for too long
            if results.should_publish() { break }
            match select.ready_timeout(Duration::from_millis(20)) {
                Err(ReadyTimeoutError) => stats.empty_select += 1,
                Ok(index) => {
                    stats.selected_count += 1;
                    let t = Instant::now();
                    if index == 0 {
                        match self.receiver.try_recv() {
                            Ok(cmd) => results.push(Ok(cmd)),
                            Err(TryRecvError::Disconnected) => results.push(Err(RecvError)),
                            Err(TryRecvError::Empty) => (),
                        }
                    } else {
                        let id = recv_map.get(&index).unwrap();
                        if let Some(machine) = self.machines.get(id) {
                            match machine.try_recv_task() {
                                None => (),
                                Some(task) => self.run_queue.push(task),
                            }
                        }
                        select.remove(index);
                        recv_map.remove(&index);
                    }
                    stats.select_time += t.elapsed();
                }
            }
        }
        results.unwrap()
    }

    // insert a machine into the machines map
    fn insert_machine(&mut self, machine: SharedCollectiveAdapter, stats: &mut SelectStats) {
        let t = Instant::now();
        let id = machine.id;
        machine.state.set(CollectiveState::RecvBlock);
        self.machines.insert(id, machine);
        stats.new_time += t.elapsed();
    }

    // create a select list from machines that are ready to receive. Much
    // as we'd like to cleanup the dead machines here, the borrow checker
    // will start complaining if we do. Instead, return dead machines
    // as results to be later processed.
    fn build_select_from_ready(&self, stats: &mut SelectStats) -> (crossbeam::Select, HashMap<usize,u128>, Vec<Result<SchedCmd, crossbeam::RecvError>> )
    {
        let t = Instant::now();
        let mut sel = crossbeam::Select::new();
        sel.recv(&self.receiver);
        let mut recv_map: HashMap<usize,u128> = HashMap::with_capacity(self.machines.len());
        // while building select, bring out the dead to be cleaned up...
        let mut results: Vec<Result<SchedCmd, RecvError>> = Vec::with_capacity(20);

        for (id, machine) in &self.machines {
            match machine.get_state() {
                CollectiveState::RecvBlock => {
                    //let idx = machine.sel_recv(&mut select);
                    let idx = machine.sel_recv(&mut sel);
                    recv_map.insert(idx, *id);
                },
                CollectiveState::Dead => results.push(Ok(SchedCmd::Remove(*id))),
                _ => (),
            }
        }
        stats.rebuild_time += t.elapsed();
        (sel, recv_map, results)
    }
}

// Encapsulation of results and the schedule for delivering them. This
// is just a little helper class to keep things neater in the selector.
struct SchedResults {
    results: Vec<Result<SchedCmd, RecvError>>,
    epoch: Instant,
}
impl SchedResults {
    fn new() -> Self { Self {results: Vec::with_capacity(10), epoch: Instant::now() } }

    // push results onto stack, note time if its the first
    fn push(&mut self, result: Result<SchedCmd, RecvError>) {
        if self.results.is_empty() { self.epoch = Instant::now() }
        self.results.push(result);
    }

    // publish if there are results, and they've aged long enough
    fn should_publish(&mut self) -> bool {
        !self.results.is_empty() && self.epoch.elapsed() > Duration::from_millis(20)
    }

    // unwrap the object, returning the accumulated results
    fn unwrap(self) -> Vec<Result<SchedCmd, RecvError>> {
        self.results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d3_channel::*;
    use d3_instruction_sets::*;
    use d3_machine::*;
    use std::time::Duration;

    #[test]
    fn can_terminate() {
        let monitor_factory = SystemMonitorFactory::new();
        let executor_factory = SystemExecutorFactory::new();
        let scheduler_factory = new_scheduler_factory();

        let scheduler: SchedulerControlObj =
            scheduler_factory.start(monitor_factory.get_sender(), executor_factory.get_queues());
        thread::sleep(Duration::from_millis(100));
        log::info!("stopping scheduler via control");
        scheduler.stop();
        thread::sleep(Duration::from_millis(100));
    }

    // A simple Alice machine
    struct Alice {}
    impl Machine<TestMessage> for Alice {
        fn receive(&self, message: &TestMessage) {}
    }

    pub fn build_machine<T, P>(
        machine: T,
    ) -> (
        Arc<Mutex<T>>,
        Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
        SharedCollectiveAdapter,
    )
    where
        T: 'static
            + Machine<P>
            + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
        P: MachineImpl,
        <P as MachineImpl>::Adapter: MachineAdapter,
    {
        let (machine, sender, collective_adapter) =
            <<P as MachineImpl>::Adapter as MachineAdapter>::build(machine);
        //let collective_adapter = Arc::new(Mutex::new(collective_adapter));
        //Server::assign_machine(collective_adapter);
        (machine, sender, collective_adapter)
    }

    #[test]
    fn test_scheduler() {
        let (monitor_sender, monitor_receiver) = crossbeam::unbounded::<MonitorMessage>();
        let (sched_sender, sched_receiver) = crossbeam::unbounded::<SchedCmd>();
        let run_queue = Arc::new(deque::Injector::<Task>::new());
        let wait_queue = Arc::new(deque::Injector::<Task>::new());

        let thread =
            SchedulerThread::spawn(sched_receiver, monitor_sender, (run_queue, wait_queue));
        // at this point the scheduler should be running
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut senders: Vec<Sender<TestMessage>> = Vec::new();
        let mut machines: Vec<Arc<Mutex<Alice>>> = Vec::new();
        // build 5 alice machines
        for _ in 1..=5 {
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
