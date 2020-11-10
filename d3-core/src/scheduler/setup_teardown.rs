use self::{executor::*, overwatch::*, traits::*};
use super::*;
use std::thread;

// A bit of an explanation is needed here. The server state and server struct live in
// two statics: server_state and server. The server_state is an AtomicCell, which makes
// it just a bit safer in the case of some weird use case where multiple threads want
// to start and stop the server -- such as parallel testing.
//
// The server is an AtomicRefCell, and its fields all come from the ServerField enum.
// This allows for something the compiler is happy with, while at the same time providing
// a decent structure for when the server is running.
//

#[allow(non_upper_case_globals)]
static server_state: AtomicCell<ServerState> = AtomicCell::new(ServerState::Stopped);

#[allow(non_upper_case_globals)]
static server: AtomicRefCell<Server> = AtomicRefCell::new(Server {
    scheduler: ServerField::Uninitialized,
    executor: ServerField::Uninitialized,
    monitor: ServerField::Uninitialized,
});

// This is the server state
#[derive(Debug, Copy, Clone, Eq, PartialEq, SmartDefault)]
enum ServerState {
    #[default]
    Stopped,
    Initializing,
    Stopping,
    Running,
}

// These are the aforementioned server fields. The server owns the scheduler, executor and monitor.
#[derive(SmartDefault)]
enum ServerField {
    #[default]
    Uninitialized,
    Scheduler(Arc<dyn Scheduler>),
    Executor(ExecutorControlObj),
    Monitor(MonitorControlObj),
}

// The server
#[derive(SmartDefault)]
pub struct Server {
    scheduler: ServerField,
    executor: ServerField,
    monitor: ServerField,
}
impl Server {
    // assign a machine to the scheduler
    pub fn assign_machine(machine: ShareableMachine) {
        match &server.borrow().scheduler {
            ServerField::Scheduler(scheduler) => scheduler.assign_machine(machine),
            _ => log::error!("Server not running, unable to assign machine."),
        }
    }
    // add a stats sender to the system monitor
    fn add_core_stats_sender(sender: CoreStatsSender) {
        match &server.borrow().monitor {
            ServerField::Monitor(monitor) => monitor.add_sender(sender),
            _ => log::error!("Server not running, unable to add stats sender."),
        }
    }
    // remove a stats sender to the system monitor
    fn remove_core_stats_sender(sender: CoreStatsSender) {
        match &server.borrow().monitor {
            ServerField::Monitor(monitor) => monitor.remove_sender(sender),
            _ => log::error!("Server not running, unable to add stats sender."),
        }
    }
    // request stats
    fn request_stats() {
        match &server.borrow().executor {
            ServerField::Executor(executor) => executor.request_stats(),
            _ => log::error!("Server not running, unable to request executor stats."),
        }
        match &server.borrow().scheduler {
            ServerField::Scheduler(scheduler) => scheduler.request_stats(),
            _ => log::error!("Server not running, unable to request scheduler stats."),
        }
    }
    // request machine info
    fn request_machine_info() {
        match &server.borrow().scheduler {
            ServerField::Scheduler(scheduler) => scheduler.request_machine_info(),
            _ => log::error!("Server not running, unable to request machine info."),
        }
    }
    // wake executor threads
    pub fn wake_executor_threads() {
        if server_state.load() != ServerState::Running {
            return;
        }
        match &server.borrow().executor {
            ServerField::Executor(executor) => executor.wake_parked_threads(),
            _ => log::error!("Server not running, unable to wake executor threads."),
        }
    }

    pub fn get_run_queue() -> Result<TaskInjector, ()> {
        let state = server_state.load();
        if state != ServerState::Running {
            log::error!("Server not running ({:#?}), unable to obtain run_q", state);
            return Err(());
        }
        match &server.borrow().executor {
            ServerField::Executor(executor) => Ok(executor.get_run_queue()),
            _ => panic!("Server not running, unable to get executor run queue."),
        }
    }
}

/// The add_core_stats_sender function adds a sender to the list of senders receiving
/// core statistic updates.
pub fn add_core_stats_sender(sender: CoreStatsSender) { Server::add_core_stats_sender(sender); }

/// The remove_core_stats_sender function removes a sender from the list of senders receiving
/// core statistic updates.
pub fn remove_core_stats_sender(sender: CoreStatsSender) { Server::remove_core_stats_sender(sender); }

/// Request stats will request the subcomponents to send their stats now, rather than waiting
/// for their periodic sending.
pub fn request_stats_now() { Server::request_stats(); }

/// Request machine_info will request the scheduler to send machine information
pub fn request_machine_info() { Server::request_machine_info(); }

// attempt state transition
fn wait_for_ownership(curr: ServerState, new: ServerState, duration: Duration) -> Result<(), ()> {
    let start = Instant::now();
    while start.elapsed() < duration {
        if curr == server_state.compare_and_swap(curr, new) {
            return Ok(());
        }
        thread::sleep(Duration::from_nanos(50));
    }
    Err(())
}

/// The start_server function starts the server, putting it in a state where it can create machines
/// that are connected to the collective.
pub fn start_server() {
    log::info!("starting server");
    // tests sometimes run in parallel, even with --jobs 1, so we wait
    let res = wait_for_ownership(ServerState::Stopped, ServerState::Initializing, Duration::from_secs(5));
    if res.is_err() {
        log::error!("force stopping server, current state is {:#?}", server_state.load());
        stop_server();
    }
    log::info!("aquired server");
    if get_executor_count() == 0 {
        let num = num_cpus::get();
        // Give them all to the executor, everything else is low-cost overhead
        set_executor_count(num);
        log::info!("setting executor count to {}", num);
    }
    let monitor_factory = SystemMonitorFactory::new();
    let executor_factory = SystemExecutorFactory::new();
    let scheduler_factory = sched_factory::create_sched_factory();
    executor_factory.with_workers(get_executor_count());

    let executor = executor_factory.start(monitor_factory.get_sender(), scheduler_factory.get_sender());
    let monitor = monitor_factory.start(Arc::clone(&executor));
    let scheduler = scheduler_factory.start(monitor_factory.get_sender(), executor_factory.get_queues());

    let mut s = server.borrow_mut();
    s.monitor = ServerField::Monitor(monitor);
    s.scheduler = ServerField::Scheduler(scheduler);
    s.executor = ServerField::Executor(executor);
    server_state.store(ServerState::Running);
    log::info!("server is now running");
}

/// The stop_server function stops the server, releasing all resources.
pub fn stop_server() {
    log::info!("stopping server");
    let state = server_state.compare_and_swap(ServerState::Running, ServerState::Stopping);
    if state != ServerState::Running {
        return;
    }
    // borrow the server to stop the subsystems, drop it before the borrow_mut()
    let borrow = server.borrow();
    if let ServerField::Executor(executor) = &borrow.executor {
        executor.stop();
        // give the executor some time to stop threads.
        thread::sleep(Duration::from_millis(20));
    }
    if let ServerField::Scheduler(scheduler) = &borrow.scheduler {
        scheduler.stop()
    }
    if let ServerField::Monitor(monitor) = &borrow.monitor {
        monitor.stop()
    }
    drop(borrow);

    let mut s = server.borrow_mut();
    s.scheduler = ServerField::Uninitialized;
    s.executor = ServerField::Uninitialized;
    s.monitor = ServerField::Uninitialized;

    server_state.store(ServerState::Stopped);
    log::info!("server is now stopped");
}

#[doc(hidden)]
#[allow(dead_code, non_upper_case_globals)]
pub static executor_count: AtomicCell<usize> = AtomicCell::new(0);

/// The get_executor_count returns the number of executor threads.
#[allow(dead_code, non_upper_case_globals)]
pub fn get_executor_count() -> usize { executor_count.load() }

/// The set_executor_count sets the number of executor threads.
/// This should be performed prior to starting the server.
#[allow(dead_code, non_upper_case_globals)]
pub fn set_executor_count(new: usize) { executor_count.store(new); }

#[cfg(test)]
pub mod tests {
    use super::*;
    use simplelog::*;
    use std::panic;

    // common function for wrapping a test with setup/teardown logic
    pub fn run_test<T>(test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        // install a simple logger
        CombinedLogger::init(vec![TermLogger::new(LevelFilter::Error, Config::default(), TerminalMode::Mixed)]).unwrap();
        setup();

        let result = panic::catch_unwind(|| test());

        teardown();
        assert!(result.is_ok())
    }

    fn setup() {
        info!("starting server");
        start_server()
    }

    fn teardown() {
        info!("stopping server");
        stop_server()
    }

    #[test]
    fn test_stop() {
        run_test(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
        });
    }
}
