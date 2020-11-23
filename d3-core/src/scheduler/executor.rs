use self::traits::*;
use super::*;
use crossbeam::deque;

const WORKER_MESSAGE_QUEUE_COUNT: usize = 10;

/// The static TIMESLICE_IN_MILLIS is the timeslice that a machine is allowed to use before
/// being returned to the scheduler. While processing the message queue, if the machine
/// has not exhausted its timeslice, it will receive additional instructions from the
/// Receiver before being returned t the scheduler. The current value is 20ms
static TIMESLICE_IN_MILLIS: AtomicCell<usize> = AtomicCell::new(20);
/// The get_time_slice function returns the current timeslice value.
pub fn get_time_slice() -> std::time::Duration { std::time::Duration::from_millis(TIMESLICE_IN_MILLIS.load() as u64) }
/// The set_time_slice function sets the current timeslice value. This should be
/// performed before starting the server.
pub fn set_time_slice(new: std::time::Duration) { TIMESLICE_IN_MILLIS.store(new.as_millis() as usize) }

/// The RUN_QUEUE_LEN static is the current length of the run queue, it is considered read-only..
pub static RUN_QUEUE_LEN: AtomicUsize = AtomicUsize::new(0);
pub fn get_run_queue_len() -> usize { RUN_QUEUE_LEN.load(Ordering::SeqCst) }
/// The EXECUTORS_SNOOZING static is the current number of executors that are idle, it is considered read-only.
pub static EXECUTORS_SNOOZING: AtomicUsize = AtomicUsize::new(0);
pub fn get_executors_snoozing() -> usize { EXECUTORS_SNOOZING.load(Ordering::SeqCst) }

// Unlike most of the system, which uses u128 ids, the executor uses usize. If atomic u128 were
// available, it would likely use u128 as well. The decision to use atomic is based upon this
// being the place where threads are used, including outside threads, such as the system monitor.

// The factory for the executor
pub struct SystemExecutorFactory {
    workers: RefCell<usize>,
    run_queue: ExecutorInjector,
}
impl SystemExecutorFactory {
    // expose the factory as a trait object.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> ExecutorFactoryObj {
        Arc::new(Self {
            workers: RefCell::new(4),
            run_queue: new_executor_injector(),
        })
    }
}

// impl the factory
impl ExecutorFactory for SystemExecutorFactory {
    // change the number of executors
    fn with_workers(&self, workers: usize) { self.workers.replace(workers); }
    // get thread run_queue
    fn get_run_queue(&self) -> ExecutorInjector { Arc::clone(&self.run_queue) }
    // start the executor
    fn start(&self, monitor: MonitorSender, scheduler: SchedSender) -> ExecutorControlObj {
        let workers: usize = *self.workers.borrow();
        let res = Executor::new(workers, monitor, scheduler, self.get_run_queue());
        Arc::new(res)
    }
}

// The notifier
struct Notifier {
    monitor: MonitorSender,
    scheduler: SchedSender,
}
impl ExecutorNotifier for Notifier {
    fn notify_parked(&self, executor_id: usize) {
        if self.monitor.send(MonitorMessage::Parked(executor_id)).is_err() {
            log::debug!("failed to notify monitor")
        };
    }
    fn notify_can_schedule_sender(&self, machine_key: usize) {
        if self.scheduler.send(SchedCmd::SendComplete(machine_key)).is_err() {
            log::warn!("failed to send to scheduler");
        }
    }
    fn notify_can_schedule_receiver(&self, machine_key: usize) {
        if self.scheduler.send(SchedCmd::RecvBlock(machine_key)).is_err() {
            log::warn!("failed to send to scheduler");
        }
    }
}

// This is the model I've adopted for managing worker threads. The thread
// data needs to be built in a way that it can be moved as a self -- it just
// makes things easier.
//
// There's an assocative struct, which is pretty thin and just caries the
// join handler and sender to control the thread.
#[allow(dead_code)]
struct ThreadData {
    id: Id,
    receiver: SchedReceiver,
    monitor: MonitorSender,
    scheduler: SchedSender,
    workers: Workers,
    run_queue: ExecutorInjector,
    work: ExecutorWorker,
    stealers: ExecutorStealers,
    shared_info: Arc<SharedExecutorInfo>,
    blocked_sender_count: usize,
}
impl ThreadData {
    /// build stealers, the workers are a shared RwLock object. Clone
    /// the stealer from every worker except ourself.
    fn build_stealers(&self) -> ExecutorStealers {
        let stealers = self
            .workers
            .read()
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
            let mut stats = ExecutorStats {
                id: self.id,
                ..Default::default()
            };
            let mut stats_event = SimpleEventTimer::default();
            let time_slice = get_time_slice();
            let backoff = LinearBackoff::new();
            log::debug!("executor {} is alive", self.id);
            let mut last_try_recv = Instant::now();
            'terminate: loop {
                // report to the system monitor, maybe look at fixing drift
                if stats_event.check() && self.monitor.send(MonitorMessage::ExecutorStats(stats)).is_err() {
                    log::debug!("failed to send exec stats to monitor");
                }
                if last_try_recv.elapsed() >= Duration::from_secs(2) {
                    last_try_recv += Duration::from_secs(2);
                    // processs any received commands, break out of loop if told to terminate
                    if self.try_recv(&stats) {
                        break 'terminate;
                    }
                }

                // move blocked senders along...
                if self.blocked_sender_count != 0 {
                    self.blocked_sender_count = self.try_completing_send(&mut stats);
                }
                // try to run a task
                let ran_task = if self.get_state() == ExecutorState::Running {
                    self.run_task(time_slice, &mut stats)
                } else {
                    false
                };

                // if no longer running and we don't have any blocked senders, we can leave
                if self.get_state() == ExecutorState::Parked {
                    tls_executor_data.with(|t| {
                        let tls = t.borrow();
                        self.blocked_sender_count = tls.blocked_senders.len();
                    });
                    if self.blocked_sender_count == 0 {
                        break;
                    }
                }
                // and after all that, we've got nothing left to do. Let's catch some zzzz's
                if self.blocked_sender_count == 0 && !ran_task {
                    if backoff.is_completed() {
                        log::trace!("executor {} is sleeping", self.id);
                        let start = std::time::Instant::now();
                        let park_duration = stats_event.remaining();
                        // sanity bailout
                        last_try_recv = start;
                        if self.try_recv(&stats) {
                            break 'terminate;
                        }
                        if RUN_QUEUE_LEN.load(Ordering::SeqCst) == 0 {
                            EXECUTORS_SNOOZING.fetch_add(1, Ordering::SeqCst);
                            thread::park_timeout(park_duration);
                            EXECUTORS_SNOOZING.fetch_sub(1, Ordering::SeqCst);
                            stats.sleep_count += 1;
                            stats.sleep_time += start.elapsed();
                        }
                        log::trace!("executor {} is awake", self.id);
                    } else {
                        backoff.snooze();
                    }
                } else if backoff.reset() {
                    stats.disturbed_nap += 1;
                }
            }
            log::debug!("executor {} is dead", self.id);
            log::debug!("{:#?}", stats);
            let remaining_tasks = self.work.len();
            if remaining_tasks > 0 {
                log::debug!(
                    "exec {} exiting with {} tasks in the worker q, will re-inject",
                    self.id,
                    remaining_tasks
                );
                // re-inject any remaining tasks
                while let Some(task) = self.work.pop() {
                    self.run_queue.push(task);
                }
            }
            tls_executor_data.with(|t| {
                let tls = t.borrow_mut();
                if !tls.blocked_senders.is_empty() {
                    log::error!(
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
    fn get_state(&self) -> ExecutorState { self.shared_info.get_state() }

    // one time setup
    fn setup(&mut self) {
        // setup TLS, to be use when blocking
        let notifier = Notifier {
            monitor: self.monitor.clone(),
            scheduler: self.scheduler.clone(),
        };
        tls_executor_data.with(|t| {
            let mut tls = t.borrow_mut();
            tls.id = self.id;
            tls.shared_info = Arc::clone(&self.shared_info);
            tls.notifier = ExecutorDataField::Notifier(Arc::new(notifier));
            tls.run_queue = ExecutorDataField::RunQueue(Arc::clone(&self.run_queue));
        });
        // pull out some commonly used stuff
        self.stealers = self.build_stealers();
        log::debug!("executor {} running with {} stealers", self.id, self.stealers.len());
        self.shared_info.set_state(ExecutorState::Running);
    }

    // check the channel, return true to quit.
    fn try_recv(&mut self, stats: &ExecutorStats) -> bool {
        let mut should_terminate = false;
        match self.receiver.try_recv() {
            Ok(SchedCmd::Terminate(_)) => should_terminate = true,
            Ok(SchedCmd::RequestStats) => {
                if self.monitor.send(MonitorMessage::ExecutorStats(*stats)).is_err() {
                    log::debug!("failed to send to monitor");
                }
            },
            // rebuild if we're running
            Ok(SchedCmd::RebuildStealers) if self.get_state() == ExecutorState::Running => {
                self.stealers = self.build_stealers();
                log::debug!(
                    "executor {} rebuild stealers, running with {} stealers",
                    self.id,
                    self.stealers.len()
                );
            },
            // ignore rebuilds when not running
            Ok(SchedCmd::RebuildStealers) => (),
            Ok(_) => log::warn!("executor received unexpected message"),
            Err(crossbeam::channel::TryRecvError::Disconnected) => should_terminate = true,
            Err(_) => (),
        };
        should_terminate
    }

    fn try_completing_send(&mut self, stats: &mut ExecutorStats) -> usize {
        tls_executor_data.with(|t| {
            let mut tls = t.borrow_mut();
            let blocked_sender_count = if !tls.blocked_senders.is_empty() {
                let len = tls.blocked_senders.len();
                if len > stats.max_blocked_senders {
                    stats.max_blocked_senders = len;
                }
                // log::trace!("exec {} blocked {} enter", self.id, len);
                let mut still_blocked: Vec<MachineSenderAdapter> = Vec::with_capacity(tls.blocked_senders.len());
                for mut sender in tls.blocked_senders.drain(..) {
                    match sender.try_send() {
                        Ok(receiver_key) => {
                            // log::trace!("exec {} send for machine {} into machine {}", self.id, sender.get_key(), receiver_key);
                            if self.scheduler.send(SchedCmd::SendComplete(sender.get_key())).is_err() {
                                log::warn!("failed to send to scheduler");
                            }
                            if self.scheduler.send(SchedCmd::RecvBlock(receiver_key)).is_err() {
                                log::warn!("failed to send to scheduler");
                            }
                        },
                        Err(TrySendError::Disconnected) => (),
                        Err(TrySendError::Full) => {
                            still_blocked.push(sender);
                        },
                    };
                }
                tls.blocked_senders = still_blocked;
                tls.last_blocked_send_len = tls.blocked_senders.len();
                // log::trace!("exec {} blocked {} exit", self.id, tls.last_blocked_send_len);
                tls.last_blocked_send_len
            } else {
                0
            };
            blocked_sender_count
        })
    }

    // if we're still running, run get a task and run it. If no tasks, yeild
    fn run_task(&mut self, time_slice: Duration, stats: &mut ExecutorStats) -> bool {
        if let Some(task) = find_task(&self.work, &self.run_queue, &self.stealers) {
            RUN_QUEUE_LEN.fetch_sub(1, Ordering::SeqCst);
            // stats.time_on_queue += task.elapsed();
            // log::trace!("exec {} task for machine {}", self.id, task.machine.get_key());

            // setup TLS in case we have to park
            let machine = task;
            let task_id = machine.get_task_id();
            let default_machine = tls_executor_data.with(|t| {
                let mut tls = t.borrow_mut();
                let default_machine = tls.machine.to_owned();
                tls.machine = Arc::clone(&machine);
                tls.task_id = task_id;
                default_machine
            });

            // log::trace!("exec {} run_q {}, begin recv machine {}", self.id, task.id, machine.get_key());
            let t = Instant::now();
            machine.receive_cmd(&machine, time_slice, stats);
            stats.recv_time += t.elapsed();
            if machine.get_state() == MachineState::SendBlock {
                stats.blocked_senders += 1;
            }
            // log::trace!("exec {} run_q {}, end recv machine {}", self.id, task.id, machine.get_key());

            // reset TLS now that we're done with the task
            tls_executor_data.with(|t| {
                let mut tls = t.borrow_mut();
                tls.machine = default_machine;
                tls.task_id = 0;
                self.blocked_sender_count = tls.blocked_senders.len();
            });

            machine.clear_task_id();
            self.reschedule(machine);
            if self.shared_info.get_state() == ExecutorState::Parked {
                log::debug!("parked executor {} completed", self.id);
            }

            // since we did a bunch of work we can leave
            true
        } else {
            false
        }
    }

    fn reschedule(&self, machine: ShareableMachine) {
        // handle cases in which we'll not reschedule
        match machine.get_state() {
            MachineState::Dead => {
                let cmd = SchedCmd::Remove(machine.get_key());
                if self.scheduler.send(cmd).is_err() {
                    log::info!("failed to send cmd to scheduler")
                }
                return;
            },
            MachineState::Running => (),
            MachineState::SendBlock => return,
            state => log::warn!("reschedule unexpected state {:#?}", state),
        }
        // mark it RecvBlock, and then check if it should be scheduled
        if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::RecvBlock) {
            log::error!(
                "exec {} machine {} expected state Running, found {:#?}",
                self.id,
                machine.get_key(),
                state
            );
        }
        // if disconnected or if the channel has data, try to schedule
        if (!machine.is_channel_empty() || machine.is_disconnected())
            && machine
                .compare_and_exchange_state(MachineState::RecvBlock, MachineState::Ready)
                .is_ok()
        {
            // log::trace!("reschedule machine {} q_len {} is_disconnected {}",
            // machine.get_key(), machine.channel_len(), machine.is_disconnected());
            schedule_machine(machine, &self.run_queue);
        }
    }
}

// The worker associated with the executor
struct Worker {
    id: Id,
    sender: SchedSender,
    stealer: Arc<deque::Stealer<ShareableMachine>>,
    thread: Option<std::thread::JoinHandle<()>>,
    // shared with the thread
    shared_info: Arc<SharedExecutorInfo>,
}
impl Worker {
    fn get_state(&self) -> ExecutorState { self.shared_info.get_state() }
    fn wake_executor(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }
    fn wakeup_and_die(&self) {
        if self.sender.send(SchedCmd::Terminate(false)).is_err() {
            log::trace!("Failed to send terminate to executor {}", self.id);
        }
        // in case its asleep, wake it up to handle the terminate message
        self.wake_executor();
    }
}
type Id = usize;
type Workers = Arc<RwLock<HashMap<Id, Worker>>>;

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.wake_executor();
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
    fn remove_worker(&mut self, dead_count: usize) { self.max_dead_executors = usize::max(self.max_dead_executors, dead_count); }
}

impl Drop for BigExecutorStats {
    fn drop(&mut self) {
        log::info!("{:#?}", self);
    }
}

// There one queue managed here, the run_queue, which contains runnable tasks.
// Executors pull from the run queue and may send to the scheduler channel,
// essentially making the message queue a wait queue.
struct Executor {
    worker_count: usize,
    monitor: MonitorSender,
    scheduler: SchedSender,
    next_worker_id: AtomicUsize,
    run_queue: ExecutorInjector,
    workers: Workers,
    parked_workers: Workers,
    barrier: Mutex<()>,
    stats: Mutex<BigExecutorStats>,
}

impl Executor {
    // create the executors
    fn new(worker_count: usize, monitor: MonitorSender, scheduler: SchedSender, run_queue: ExecutorInjector) -> Self {
        log::info!("Starting executor with {} executors", worker_count);
        let factory = Self {
            worker_count,
            monitor,
            scheduler,
            next_worker_id: AtomicUsize::new(1),
            run_queue,
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
        // tell all the workers to stop their executors
        for w in self.workers.read().iter() {
            w.1.wakeup_and_die();
        }
    }
    // notification that an executor is parked
    fn parked_executor(&self, id: usize) {
        // protect ourself from re-entry
        let _guard = self.barrier.lock();
        // dump state of all workers
        self.workers.read().iter().for_each(|(_, v)| {
            let state = v.get_state();
            log::debug!("worker {} {:#?}", v.id, state)
        });

        if let Some(worker) = self.workers.read().get(&id) {
            // at this point the worker thread won't load tasks into local queue, so drain it
            let mut count = 0;
            loop {
                match worker.stealer.steal() {
                    deque::Steal::Empty => break,
                    deque::Steal::Retry => (),
                    deque::Steal::Success(task) => {
                        count += 1;
                        self.run_queue.push(task)
                    },
                }
            }
            log::debug!("stole back {} tasks queue is_empty() = {}", count, self.run_queue.is_empty());
        }

        if let Some(worker) = self.workers.write().remove(&id) {
            // the executor will self-terminate
            // save the worker, otherwise it gets dropped things go wrong with join
            self.parked_workers.write().insert(id, worker);
            let dead_count = self.parked_workers.read().len();
            self.stats.lock().remove_worker(dead_count);
        }
        self.add_executor();
    }
    // wake parked threads
    fn wake_parked_threads(&self) {
        // protect ourself from re-entry
        let _guard = self.barrier.lock();
        // tell the workers to wake their executor
        self.workers.read().iter().for_each(|(_, v)| {
            v.wake_executor();
        });
    }
    // request stats
    fn request_stats(&self) {
        self.workers.read().iter().for_each(|(_, v)| {
            if v.sender.send(SchedCmd::RequestStats).is_err() {
                log::debug!("failed to send to executor")
            }
        });
    }
    // get run_queue
    fn get_run_queue(&self) -> ExecutorInjector { Arc::clone(&self.run_queue) }

    // notification that an executor completed and can be joined
    fn joinable_executor(&self, id: usize) {
        if let Some(_worker) = self.parked_workers.write().remove(&id) {
            log::debug!("dropping worker {}", id);
        } else if self.workers.read().contains_key(&id) {
            log::debug!("dropping worker {} is still in the workers table", id);
        } else {
            log::warn!("joinable executor {} isn't on any list", id);
        }
        let live_count = self.workers.read().len();
        log::debug!("there are now {} live executors", live_count);
        self.wake_parked_threads();
    }

    // dynamically add an executor
    fn add_executor(&self) {
        let (worker, thread_data) = self.new_worker();
        self.workers.write().insert(worker.id, worker);
        let live_count = self.workers.read().len();
        self.stats.lock().add_worker(live_count);
        let id = thread_data.id;
        self.workers.write().get_mut(&id).unwrap().thread = thread_data.spawn();
        self.workers.read().iter().for_each(|w| {
            if w.1.sender.send(SchedCmd::RebuildStealers).is_err() {
                log::debug!("failed to send to executor");
            }
        });
    }

    // create a new worker and executor
    fn new_worker(&self) -> (Worker, ThreadData) {
        let id = self.next_worker_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = crossbeam::channel::bounded::<SchedCmd>(WORKER_MESSAGE_QUEUE_COUNT);
        let work = new_executor_worker();
        let stealer = Arc::new(work.stealer());
        let worker = Worker {
            id,
            sender,
            stealer,
            thread: None,
            shared_info: Arc::new(SharedExecutorInfo::default()),
        };
        let data = ThreadData {
            id,
            receiver,
            monitor: self.monitor.clone(),
            scheduler: self.scheduler.clone(),
            run_queue: Arc::clone(&self.run_queue),
            work,
            workers: Arc::clone(&self.workers),
            stealers: Vec::with_capacity(8),
            shared_info: Arc::clone(&worker.shared_info),
            blocked_sender_count: 0,
        };
        (worker, data)
    }

    // lauch the workers and get the executors running
    fn launch(&self) {
        let mut threads = Vec::<ThreadData>::new();
        for _ in 0 .. self.worker_count {
            let (worker, thread_data) = self.new_worker();
            self.workers.write().insert(worker.id, worker);
            threads.push(thread_data);
        }
        for thread in threads {
            let id = thread.id;
            self.workers.write().get_mut(&id).unwrap().thread = thread.spawn();
        }
    }
}

// impl the trait object for controlling the executor
impl ExecutorControl for Executor {
    /// Notification that an executor has been parked
    fn parked_executor(&self, id: usize) { self.parked_executor(id); }
    /// notifies the executor that an executor completed and can be joined
    fn joinable_executor(&self, id: usize) { self.joinable_executor(id); }
    /// stop the executor
    fn stop(&self) { self.stop(); }
    /// Wake parked threads
    fn wake_parked_threads(&self) { self.wake_parked_threads(); }
    /// request stats from executor
    fn request_stats(&self) { self.request_stats(); }
    /// get run_queue
    fn get_run_queue(&self) -> ExecutorInjector { self.get_run_queue() }
}

impl Drop for Executor {
    fn drop(&mut self) {
        log::info!("sending terminate to all workers");
        for w in self.workers.write().iter() {
            if w.1.sender.send(SchedCmd::Terminate(false)).is_err() {
                log::trace!("Failed to send terminate to worker");
            }
        }
        log::info!("synchronizing worker thread shutdown");
        for w in self.workers.write().iter_mut() {
            if let Some(thread) = w.1.thread.take() {
                if thread.join().is_err() {
                    log::debug!("failed to join executor")
                }
            }
        }
        log::info!("dropped thread pool");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    use self::overwatch::SystemMonitorFactory;
    use self::sched_factory::create_sched_factory;

    #[test]
    fn can_terminate() {
        let monitor_factory = SystemMonitorFactory::new();
        let executor_factory = SystemExecutorFactory::new();
        let scheduler_factory = create_sched_factory();
        executor_factory.with_workers(16);
        let executor = executor_factory.start(monitor_factory.get_sender(), scheduler_factory.get_sender());
        thread::sleep(Duration::from_millis(100));
        executor.stop();
        thread::sleep(Duration::from_millis(100));
    }
}
