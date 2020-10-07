
use super::*;
use crossbeam::deque;
use self::{traits::*};

const WORKER_MESSAGE_QUEUE_COUNT: usize = 10;

///
/// Unlike most of the system, which uses u128 ids, the executor uses usize. If atomic u128 were
/// available, it would likely use u128 as well. The decision to use atomic is based upon this
/// being the place where threads are used, including outside threads, such as the system monitor.

/// The factory for the executor
pub struct SystemExecutorFactory {
    workers: RefCell<usize>,
    run_queue: TaskInjector,
    wait_queue: SchedTaskInjector,
}
impl SystemExecutorFactory {
    // expose the factory as a trait object.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> ExecutorFactoryObj {
        Arc::new(Self {
            workers: RefCell::new(4),
            run_queue: Arc::new(deque::Injector::<Task>::new()),
            wait_queue: Arc::new(deque::Injector::<SchedTask>::new()),
        })
    }
}

/// impl the factory
impl ExecutorFactory for SystemExecutorFactory {
    // change the number of executors
    fn with_workers(&self, workers: usize) {
        self.workers.replace(workers);
    }
    // get thread run_queue, wait_queue
    fn get_queues(&self) -> (TaskInjector, SchedTaskInjector) {
        (Arc::clone(&self.run_queue), Arc::clone(&self.wait_queue))
    }
    // start the executor
    fn start(&self, monitor: MonitorSender, scheduler: SchedSender) -> ExecutorControlObj {
        let workers: usize = *self.workers.borrow();
        let res = Executor::new(workers, monitor, scheduler, self.get_queues());
        Arc::new(res)
    }
}

/// Find a machine to receive a message.
fn find_task<T>(
    local: &deque::Worker<T>,
    global: &deque::Injector<T>,
    stealers: &[Arc<deque::Stealer<T>>],
) -> Option<T> {
    // Pop a task from the local queue, if not empty.
    local.pop().or_else(|| {
        // Otherwise, we need to look for a task elsewhere.
        iter::repeat_with(|| {
            // Try stealing a batch of tasks from the global queue.
            global
                .steal_batch_and_pop(local)
                // Or try stealing a task from one of the other threads.
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        // Loop while no task was stolen and any steal operation needs to be retried.
        .find(|s| !s.is_retry())
        // Extract the stolen task, if there is one.
        .and_then(|s| s.success())
    })
}


/// The notifier
struct Notifier {
    monitor: MonitorSender,
    scheduler: SchedSender,
    wait_queue: SchedTaskInjector,
}
impl ExecutorNotifier for Notifier {
    fn notify_parked(&self, executor_id: usize) {
        if self.monitor
            .send(MonitorMessage::Parked(executor_id))
            .is_err(){};
    }
    fn notify_can_schedule(&self, machine_key: usize) {
        self.wait_queue.push(SchedTask::new(machine_key));
    }
}

///
/// This is the model I've adopted for managing worker threads. The thread
/// data needs to be built in a way that it can be moved as a self -- it just
/// makes things easier.
///
/// There's an assocative struct, which is pretty thin and just caries the
/// join handler and sender to control the thread.
#[allow(dead_code)]
struct ThreadData {
    id: Id,
    receiver: SchedReceiver,
    monitor: MonitorSender,
    scheduler: SchedSender,
    workers: Workers,
    run_queue: Arc<deque::Injector<Task>>,
    wait_queue: Arc<deque::Injector<SchedTask>>,
    work: deque::Worker<Task>,
    stealers: Stealers,
    shared_info: Arc<Mutex<SharedExecutorInfo>>,
}
impl ThreadData {
    /// build stealers, the workers are a shared RwLock object. Clone
    /// the stealer from every worker except ourself.
    fn build_stealers(&self) -> Stealers {
        let stealers = self
            .workers
            .read()
            .unwrap()
            .iter()
            .filter(|w| w.1.id != self.id)
            .map(|w| Arc::clone(&w.1.stealer))
            .collect();
        stealers
    }


    // This is the executor thread. It has a few responsibilites. It listens for,
    // and acts upon events (via channel). It complete senders that are blocked.
    // It gets tasks and runs them. It might be better to have a select
    // for blocked senders, or aggregate them and have a separate thread responsible
    // for them. In a well tuned system, we shouldn't see blocking of senders,
    // which represents a failure in dataflow management.
    fn spawn(mut self) -> Option<std::thread::JoinHandle<()>> {
        let thread = std::thread::spawn(move || {
            self.setup();
            let mut stats = ExecutorStats::default();
            stats.id = self.id;
            let mut last_stats_time = std::time::Instant::now();
            let time_slice = get_time_slice();
            loop {
                // report to the system monitor, maybe look at fixing drift
                if last_stats_time.elapsed().as_millis() > 1000*60*5 {
                    self.monitor
                        .send(MonitorMessage::ExecutorStats(stats))
                        .unwrap();
                    last_stats_time = std::time::Instant::now();
                }
                // processs any received commands, break out of loop if told to terminate
                if self.try_recv() { break }
                // move blocked senders along...
                self.try_completing_send(&mut stats);
                // try to run a task
                self.run_task(time_slice, &mut stats);
                // if no longer running and we don't have any blocked senders, we can leave
                if self.get_state() == ExecutorState::Parked {
                    let is_empty = tls_executor_data.with(|t| {
                        let tls = t.borrow();
                        tls.blocked_senders.is_empty()
                    });
                    if is_empty { break }
                }
            }
            log::debug!("executor {} is done", self.id);
            log::debug!("{:#?}", stats);
            tls_executor_data.with(|t| {
                let tls = t.borrow_mut();
                if !tls.blocked_senders.is_empty() {
                    log::warn!(
                        "executor {} exited, but continues to have {} blocked senders",
                        self.id,
                        tls.blocked_senders.len()
                    );
                }
            });
            // as a last and final, tell the monitor that we're dead...
            if self.monitor.send(MonitorMessage::Terminated(self.id)).is_err() {
                log::warn!("executor {} exiting without informing system monitor", self.id);
            }
        });
        Some(thread)
    }

    #[inline]
    // get the executor state
    fn get_state(&self) -> ExecutorState {
        self.shared_info.lock().unwrap().get_state()
    }

    // one time setup
    fn setup(&mut self) {
        // setup TLS, to be use when blocking
        let notifier = Notifier {
            monitor: self.monitor.clone(),
            scheduler: self.scheduler.clone(),
            wait_queue: Arc::clone(&self.wait_queue),
        };
        tls_executor_data.with(|t| {
            let mut tls = t.borrow_mut();
            tls.id = self.id;
            tls.shared_info = Arc::clone(&self.shared_info);
            tls.notifier = ExecutorDataField::Notifier(Arc::new(notifier));
        });            
        // pull out some commonly used stuff
        self.stealers = self.build_stealers();
        log::debug!("executor {} running with {} stealers", self.id, self.stealers.len());
        self.shared_info.lock().as_mut().unwrap().set_state(ExecutorState::Running);
    }

    // check the channel, return true to quit.
    fn try_recv(&mut self) -> bool {
        let mut should_terminate = false;
        match self.receiver.try_recv() {
            Ok(SchedCmd::Terminate(_)) => should_terminate = true,
            // rebuild if we're running
            Ok(SchedCmd::RebuildStealers) if self.get_state() == ExecutorState::Running => {
                self.stealers = self.build_stealers();
                log::debug!("executor {} rebuild stealers, running with {} stealers", self.id, self.stealers.len());
            },
            // ignore rebuilds when not running
            Ok(SchedCmd::RebuildStealers) => (),
            Ok(_) => log::warn!("executor received unexpected message"),
            Err(crossbeam::TryRecvError::Disconnected) => should_terminate = true,
            Err(_) => (),
        };
        should_terminate
    }

    fn try_completing_send(&mut self, stats: &mut ExecutorStats) {
        tls_executor_data.with(|t| {
            let mut tls = t.borrow_mut();
            if !tls.blocked_senders.is_empty() {
                let len = tls.blocked_senders.len();
                if len > stats.max_blocked_senders {
                    stats.max_blocked_senders = len;
                }
                stats.blocked_senders += (len - tls.last_blocked_send_len) as u128;
                let mut still_blocked: Vec<SharedCollectiveSenderAdapter> =
                    Vec::with_capacity(tls.blocked_senders.len());
                for mut sender in tls.blocked_senders.drain(..) {
                    match sender.try_send() {
                        Ok(()) => {
                            self.wait_queue.push(SchedTask::new(sender.get_key()));
                        },
                        Err(TrySendError::Disconnected) => {},
                        Err(TrySendError::Full) => {
                            still_blocked.push(sender);
                        },
                    };
                }
                tls.blocked_senders = still_blocked;
                tls.last_blocked_send_len = tls.blocked_senders.len();
            }
        });
    }

    // if we're stull running, run get a task and run it. If no tasks, yeild
    fn run_task(&mut self, time_slice: Duration, stats: &mut ExecutorStats) {
        if self.get_state() == ExecutorState::Running {
            if let Some(task) = find_task(&self.work, &self.run_queue, &self.stealers) {
                stats.time_on_queue += task.start.elapsed();
                let t = self.shared_info.lock().as_mut().unwrap().set_idle();
                // setup TLS in case we have to park
                let machine = task.machine;
                tls_executor_data.with(|t| {
                    let mut tls = t.borrow_mut();
                    tls.machine = ExecutorDataField::Machine(Arc::clone(&machine))
                });
                machine.receive_cmd( time_slice, stats);
                stats.recv_time += t.elapsed();
                self.shared_info.lock().as_mut().unwrap().set_idle();
                self.reschedule(machine);
                if self.shared_info.lock().unwrap().get_state() == ExecutorState::Parked {
                    log::debug!("parked executor {} completed", self.id);
                }
            } else {
                std::thread::yield_now();
            }
        }
    }

    #[inline]
    fn reschedule(&self, machine: ShareableMachine) {
        let cmd = match machine.state.get() {
            CollectiveState::Running => {self.wait_queue.push(SchedTask::new(machine.key)); return},
            CollectiveState::Dead => SchedCmd::Remove(machine.key),
            _ => return
        };
        if self.scheduler.send(cmd).is_err() { log::info!("failed to send cmd to scheduler") }
    }
}

/// The worker associated with the executor
struct Worker {
    id: Id,
    sender: SchedSender,
    stealer: Arc<deque::Stealer<Task>>,
    thread: Option<std::thread::JoinHandle<()>>,
    // shared with the thread
    shared_info: Arc<Mutex<SharedExecutorInfo>>,
}
impl Worker {
    fn get_state_and_elapsed(&self) -> (ExecutorState, Duration) {
        self.shared_info.lock().unwrap().get_state_and_elapsed()
    }
}
type Id = usize;
type Workers = Arc<RwLock<HashMap<Id, Worker>>>;
type Stealers = Vec<Arc<deque::Stealer<Task>>>;
type Injector = TaskInjector;

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::trace!("failed to join executor thread {}", self.id);
            }
        }
        log::debug!("executor {} shut down", self.id);
    }
}

#[derive(Debug, Default)]
struct BigExecutorStats {
    executors_created: usize,
    max_live_executors: usize,
    max_dead_executors: usize,
}
impl BigExecutorStats {
    fn add_worker(&mut self, live_count: usize) {
        self.executors_created += 1;
        self.max_live_executors = usize::max(self.max_live_executors, live_count);
    }
    fn remove_worker(&mut self, dead_count: usize) {
        self.max_dead_executors = usize::max(self.max_dead_executors, dead_count);
    }
}

impl Drop for BigExecutorStats {
    fn drop(&mut self) {
        log::info!("{:#?}", self);
    }
}

// There are two queues managed here, the run_queue, which contains runnable tasks
// and the wait_queue, which contains waiting tasks. Executors pull from the run queue
// and may push into the wait queue. Currently, the send a message to the scheduler,
// essentially making the message queue a wait queue. Need to run some performance
// tests to determine which is better.
struct Executor {
    worker_count: usize,
    monitor: MonitorSender,
    scheduler: SchedSender,
    next_worker_id: AtomicUsize,
    run_queue: Injector,
    wait_queue: SchedTaskInjector,
    workers: Workers,
    parked_workers: Workers,
    barrier: Mutex<()>,
    stats: Mutex<BigExecutorStats>,
}

impl Executor {
    // create the executors
    fn new(
        worker_count: usize,
        monitor: MonitorSender,
        scheduler: SchedSender,
        queues: (TaskInjector, SchedTaskInjector),
    ) -> Self {
        log::info!("Starting executor with {} executors", worker_count);
        let factory = Self {
            worker_count,
            monitor,
            scheduler,
            next_worker_id: AtomicUsize::new(1),
            run_queue: queues.0,
            wait_queue: queues.1,
            workers: Arc::new(RwLock::new(HashMap::with_capacity(worker_count))),
            parked_workers: Arc::new(RwLock::new(HashMap::with_capacity(worker_count))),
            barrier: Mutex::new(()),
            stats: Mutex::new(BigExecutorStats::default()),
        };
        factory.launch();
        factory
    }
    // stop the executor
    fn stop(&self) {
        for w in self.workers.read().unwrap().iter() {
            if w.1.sender.send(SchedCmd::Terminate(false)).is_err() {
                log::trace!("Failed to send terminate to executor {}", *w.0);
            }
        }
    }
    // notification that an executor is parked
    fn parked_executor(&self, id: usize) {
        // protect ourself from re-entry
        let _guard = self.barrier.lock().unwrap();
        // dump state of all workers
        self.workers.read().unwrap().iter().for_each(|(_,v)| {
            let (state, elapsed) = v.get_state_and_elapsed();
            log::debug!("worker {} {:#?} {:#?}", v.id, state, elapsed )
        });

        if let Some(worker) = self.workers.read().unwrap().get(&id) {
            // at this point the worker thread won't load tasks into local queue, so drain it
            let mut count = 0;
            loop {
                match worker.stealer.steal() {
                    deque::Steal::Empty => break,
                    deque::Steal::Retry => (),
                    deque::Steal::Success(task) => { count += 1; self.run_queue.push(task) },
                }
            }
            log::debug!("stole back {} tasks queue is_empty() = {}", count, self.run_queue.is_empty());
        }

        if let Some(worker) = self.workers.write().unwrap().remove(&id) {
            // the executor will self-terminate
            // save the worker, otherwise it gets dropped things go wrong with join
            self.parked_workers.write().unwrap().insert(id, worker);
            let dead_count = self.parked_workers.read().unwrap().len();
            self.stats.lock().unwrap().remove_worker(dead_count);
        }
        self.add_executor();
    }

    // notification that an executor completed and can be joined
    fn joinable_executor(&self, id: usize) {
        if let Some(_worker) = self.parked_workers.write().unwrap().remove(&id) {
            log::debug!("dropping worker {}", id);
        } else if self.workers.read().unwrap().contains_key(&id) {
            log::debug!("dropping worker {} is still in the workers table", id);
        } else {
            log::warn!("joinable executor {} isn't on any list", id);
        }
        /*
        if self.workers.read().unwrap().len() < self.worker_count {
            self.add_executor();
        }
        */
    }

    // dynamically add an executor
    fn add_executor(&self) {
        let (worker, thread_data) = self.new_worker();
        self.workers.write().unwrap().insert(worker.id, worker);
        let live_count = self.workers.read().unwrap().len();
        self.stats.lock().unwrap().add_worker(live_count);
        let id = thread_data.id;
        self.workers.write().unwrap().get_mut(&id).unwrap().thread = thread_data.spawn();
        self.workers
            .read()
            .unwrap()
            .iter()
            .for_each(|w| if w.1.sender.send(SchedCmd::RebuildStealers).is_err(){});
    }

    // create a new worker and executor
    fn new_worker(&self) -> (Worker, ThreadData) {
        let id = self.next_worker_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = crossbeam::bounded::<SchedCmd>(WORKER_MESSAGE_QUEUE_COUNT);
        let work = deque::Worker::<Task>::new_fifo();
        let stealer = Arc::new(work.stealer());
        let worker = Worker {
            id,
            sender,
            stealer,
            thread: None,
            shared_info: Arc::new(Mutex::new(SharedExecutorInfo::default())),
        };
        let data = ThreadData {
            id,
            receiver,
            monitor: self.monitor.clone(),
            scheduler: self.scheduler.clone(),
            run_queue: Arc::clone(&self.run_queue),
            wait_queue: Arc::clone(&self.wait_queue),
            work,
            workers: Arc::clone(&self.workers),
            stealers: Vec::with_capacity(8),
            shared_info: Arc::clone(&worker.shared_info),
        };
        (worker, data)
    }

    // lauch the workers and get the executors running
    fn launch(&self) {
        let mut threads = Vec::<ThreadData>::new();
        for _ in 0..self.worker_count {
            let (worker, thread_data) = self.new_worker();
            self.workers.write().unwrap().insert(worker.id, worker);
            threads.push(thread_data);
        }
        for thread in threads {
            let id = thread.id;
            self.workers.write().unwrap().get_mut(&id).unwrap().thread = thread.spawn();
        }
    }
}

/// impl the trait object for controlling the executor
impl ExecutorControl for Executor {
    /// Notification that an executor has been parked
    fn parked_executor(&self, id: usize) {
        self.parked_executor(id);
    }
    
    /// notifies the executor that an executor completed and can be joined
    fn joinable_executor(&self, id: usize) {
        self.joinable_executor(id);
    }

    /// stop the executor
    fn stop(&self) {
        self.stop();
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        log::info!("sending terminate to all workers");
        //@todo: Need to unpark any parked threads

        for w in self.workers.write().unwrap().iter() {
            if w.1.sender.send(SchedCmd::Terminate(false)).is_err() {
                log::trace!("Failed to send terminate to worker");
            }
        }
        log::info!("synchronizing worker thread shutdown");
        for w in self.workers.write().unwrap().iter_mut() {
            if let Some(thread) = w.1.thread.take() {
                thread.join().unwrap();
            }
        }
        log::info!("dropped thread pool");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overwatch::SystemMonitorFactory;
    use crate::scheduler::SystemSchedulerFactory;
    use crate::setup_teardown::tests::run_test;
    use d3_channel::*;
    use d3_instruction_sets::*;
    use d3_machine::*;
    use std::fmt;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn can_terminate() {
        let monitor_factory = SystemMonitorFactory::new();
        let executor_factory = SystemExecutorFactory::new();
        let scheduler_factory = SystemSchedulerFactory::new();
        executor_factory.with_workers(16);
        let executor =
            executor_factory.start(monitor_factory.get_sender(), scheduler_factory.get_sender());
        thread::sleep(Duration::from_millis(100));
        executor.stop();
        thread::sleep(Duration::from_millis(100));
    }

    #[test]
    fn can_receive() {
        run_test(|| {
            let f = Forwarder::default();
            let (t, s) = machine::connect(f);
            s.send(TestMessage::Test).unwrap();
            thread::sleep(Duration::from_millis(20));
            assert_eq!(t.lock().unwrap().get_and_clear_received_count(), 1);
            assert_eq!(t.lock().unwrap().get_and_clear_received_count(), 0);
        });
    }

    #[test]
    fn can_callback() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = machine::connect(f);
            let (sender, r) = channel();
            let mut data = TestStruct::default();
            data.from_id = 10000;
            s.send(TestMessage::TestCallback(sender, data)).unwrap();
            let m = r.recv_timeout(Duration::from_secs(1)).unwrap();
            match m {
                TestMessage::TestStruct(results) => {
                    assert_eq!(results.from_id, data.from_id);
                    assert_eq!(results.received_by, 1);
                }
                _ => assert_eq!(true, false),
            }
        });
    }

    #[test]
    fn can_forward() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = machine::connect(f);
            let (sender, r) = channel();
            s.send(TestMessage::AddSender(sender.clone())).unwrap();
            s.send(TestMessage::Test).unwrap();
            let m = r.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
        });
    }

    #[test]
    fn can_notify() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = machine::connect(f);
            let (sender, r) = channel();
            s.send(TestMessage::Notify(sender.clone(), 2)).unwrap();
            s.send(TestMessage::Test).unwrap();
            match r.recv_timeout(Duration::from_millis(100)) {
                Ok(m) => panic!("notification arrived early"),
                Err(_RecvTimeoutError) => (),
            }
            s.send(TestMessage::Test).unwrap();
            match r.recv_timeout(Duration::from_secs(1)) {
                Ok(m) => assert_eq!(m, TestMessage::TestData(2)),
                Err(e) => panic!("unexpected error '{}' while waiting for data", e),
            }
        });
    }

    #[test]
    fn can_fanout() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = machine::connect(f);
            let (sender1, r1) = channel();
            let (sender2, r2) = channel();
            s.send(TestMessage::AddSender(sender1.clone())).unwrap();
            s.send(TestMessage::AddSender(sender2.clone())).unwrap();
            s.send(TestMessage::Test).unwrap();
            let m = r1.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
            let m = r2.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
        });
    }

    #[test]
    fn daisy_chain() {
        // The previous tests asserted that the forward is well behaved.
        // The scheduler and executor, while not stressed, is also well behaved.
        // The following tests stress the executor in different ways

        // the daisy chain sets up a chain of forwarders, then sends a message
        // into the first, which should run through all the forwarders and end
        // with a notification.

        let machine_count = 1000;
        let messages = 50;
        let iterations = 10;

        run_test(|| {
            let mut machines: Vec<TestMessageSender> = Vec::with_capacity(machine_count);
            let mut instances: Vec<Arc<Mutex<Forwarder>>> = Vec::with_capacity(machine_count);
            let f = Forwarder::new(1);
            let (_f, s) = machine::connect(f);
            instances.push(_f);
            let first_sender = s.clone();
            let mut last_sender = s.clone();
            machines.push(s);
            for idx in 2..=machine_count {
                let (_f, s) = machine::connect(Forwarder::new(idx));
                instances.push(_f);
                last_sender.send(TestMessage::AddSender(s.clone())).unwrap();
                last_sender = s.clone();
                machines.push(s);
            }
            // turn the last into a notifier
            let (sender, receiver) = channel();
            last_sender
                .send(TestMessage::Notify(sender.clone(), messages))
                .unwrap();
            for _ in 0..iterations {
                for msg_id in 0..messages {
                    match first_sender.try_send(TestMessage::TestData(msg_id)) {
                        Ok(()) => (),
                        Err(e) => match e {
                            crossbeam::TrySendError::Full(m) => {
                                log::info!("full");
                                first_sender.send(m).unwrap();
                            }
                            crossbeam::TrySendError::Disconnected(m) => {
                                log::info!("disconnected");
                            }
                        },
                    };
                }
                log::info!("sent an iteration, waiting for response");
                match receiver.recv_timeout(Duration::from_secs(5)) {
                    Ok(m) => {
                        assert_eq!(m, TestMessage::TestData(messages));
                        log::info!("an iteration completed");
                    }
                    Err(_) => {
                        for forwarder in &instances {
                            log::info!(
                                "id {} count{}",
                                forwarder.lock().unwrap().get_id(),
                                forwarder.lock().unwrap().get_and_clear_received_count()
                            );
                        }
                    }
                };
            }
        });
    }

    #[test]
    fn fanout_fanin() {
        // create a machine which forwards any message it receives to all of the
        // other machines, that's the fanout leg. Each of those machines will forward
        // the message it receives to a single machine, that's the fanin leg.
        //
        // this is likely to strain the executors, as the fanin will leg will cause
        // senders to block on the bounded queue, which has a size of 500.
        //
        // with 1002 machines, that 1000 messages slamming into a single machine.
        // if a burst of 20 messages are sent, that's 20K messages flowing into
        // a bounded pipe. The result is going to be parking of senders and the
        // executor is going to need a means of processing other work, as it
        // can't block on the send queue.
        let machine_count = 1500;
        let messages = 200;
        let iterations = 1;
        run_test(|| {
            let mut machines: Vec<TestMessageSender> = Vec::with_capacity(machine_count);
            let (_f, fanout_sender) = machine::connect(Forwarder::new(1));
            let (_f, fanin_sender) = machine::connect(Forwarder::new(2));

            for idx in 3..=machine_count {
                let (_f, s) = machine::connect(Forwarder::new(idx));
                fanout_sender
                    .send(TestMessage::AddSender(s.clone()))
                    .unwrap();
                s.send(TestMessage::AddSender(fanin_sender.clone()))
                    .unwrap();
                machines.push(s);
            }
            // turn the fanin into a notifier
            let (sender, receiver) = channel();
            let expect_count = (machine_count - 2) * messages;
            fanin_sender
                .send(TestMessage::Notify(sender.clone(), expect_count))
                .unwrap();
            for _ in 0..iterations {
                for _ in 0..messages {
                    fanout_sender.send(TestMessage::Test).unwrap();
                }
                let m = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
                assert_eq!(m, TestMessage::TestData(expect_count));
                log::info!("an iteration completed");
            }
        });
    }

    use std::sync::Mutex;
    type TestMessageSender = Sender<TestMessage>;
    /// The Forwarder is the swiss army knife for tests. It can be a fanin, fanout, chain, or callback receiver.
    #[derive(Default)]
    pub struct Forwarder {
        id: usize,
        /// received_count is the count of messages received by this forwarder.
        received_count: AtomicUsize,
        /// The mutable bits...
        mutations: Mutex<ForwarderMutations>,
    }

    #[derive(Default)]
    pub struct ForwarderMutations {
        /// collection of senders, each will be sent any received message.
        senders: Vec<TestMessageSender>,
        /// notify_count is compared against received_count for means of notifcation.
        notify_count: usize,
        /// notify_sender is sent a TestData message with the data being the number of messages received.
        notify_sender: Option<TestMessageSender>,
        /// sequencing
        sequence: usize,
    }

    impl Forwarder {
        pub fn new(id: usize) -> Self {
            Self {
                id,
                ..Default::default()
            }
        }
        pub fn get_id(&self) -> usize {
            self.id
        }
        pub fn get_and_clear_received_count(&self) -> usize {
            let received_count = self.received_count.load(Ordering::SeqCst);
            self.received_count.store(0, Ordering::SeqCst);
            received_count
        }
    }

    impl Machine<TestMessage> for Forwarder {
        fn receive(&self, message: &TestMessage) {
            // it a bit ugly, but its also a clean way to handle the data we need to access
            let mut mutations = self.mutations.lock().unwrap();
            // handle configuation messages without bumping counters
            match message {
                TestMessage::Notify(sender, on_receive_count) => {
                    mutations.notify_sender = Some(sender.clone());
                    mutations.notify_count = *on_receive_count;
                    return;
                }
                TestMessage::AddSender(sender) => {
                    mutations.senders.push(sender.clone());
                    return;
                }
                _ => (),
            }
            self.received_count.fetch_add(1, Ordering::SeqCst);
            // forward the message
            &mutations
                .senders
                .iter()
                .for_each(|sender| sender.send(message.clone()).unwrap());
            if mutations.notify_count > 0
                && (self.received_count.load(Ordering::SeqCst) % 5000 == 0)
            {
                log::info!(
                    "rcvs {} out of {}",
                    self.received_count.load(Ordering::SeqCst),
                    mutations.notify_count
                );
            }
            // send notification if we've met the criteria
            if self.received_count.load(Ordering::SeqCst) == mutations.notify_count
                && mutations.notify_sender.is_some()
            {
                mutations
                    .notify_sender
                    .as_ref()
                    .unwrap()
                    .send(TestMessage::TestData(
                        self.received_count.load(Ordering::SeqCst),
                    ))
                    .unwrap();
                self.received_count.store(0, Ordering::SeqCst);
            }
            match message {
                TestMessage::TestCallback(sender, mut test_struct) => {
                    test_struct.received_by = self.id;
                    sender.send(TestMessage::TestStruct(test_struct)).unwrap();
                }
                TestMessage::TestData(seq) => {
                    if *seq == 0 as usize {
                        mutations.sequence = 0
                    } else {
                        mutations.sequence += 1;
                        assert_eq!(*seq, mutations.sequence);
                    }
                }
                _ => (),
            }
        }
    }

    impl fmt::Display for Forwarder {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "Forwarder {} forwarded {}",
                self.id,
                self.received_count.load(Ordering::SeqCst)
            )
        }
    }
}
