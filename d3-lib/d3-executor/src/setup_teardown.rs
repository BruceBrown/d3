use super::*;

///
/// A bit of an explanation is needed here. The server state and server struct live in
/// two statics: server_state and server. The server_state is an AtomicCell, which makes
/// it just a bit safer in the case of some weird use case where multiple threads want
/// to start and stop the server -- such as parallel testing.
///
/// The server is an AtomicRefCell, and its fields all come from the ServerField enum.
/// This allows for something the compiler is happy with, while at the same time providing
/// a decent structure for when the server is running.
/// 

#[allow(non_upper_case_globals)]
static server_state: AtomicCell<ServerState> = AtomicCell::new(ServerState::Stopped);

#[allow(non_upper_case_globals)]
static server: AtomicRefCell<Server> = AtomicRefCell::new(Server {
    scheduler: ServerField::Uninitialized,
    executor: ServerField::Uninitialized,
    monitor: ServerField::Uninitialized,
});

/// This is the server state
#[derive(Debug, Copy, Clone, Eq, PartialEq, SmartDefault)]
enum ServerState {
    #[default]
    Stopped,
    Initializing,
    Stopping,
    Running,
}

/// These are the aforementioned server fields. The server owns the scheduler, executor and monitor.
#[derive(SmartDefault)]
enum ServerField {
    #[default]
    Uninitialized,
    Scheduler(SchedulerControlObj),
    Executor(ExecutorControlObj),
    Monitor(MonitorControlObj),
}

/// The server
#[derive(SmartDefault)]
pub struct Server {
    scheduler: ServerField,
    executor: ServerField,
    monitor: ServerField,
}
impl Server {
    /// assign a machine to the scheduler
    pub fn assign_machine(machine: SharedCollectiveAdapter) {
        match &server.borrow().scheduler {
            ServerField::Scheduler(scheduler) => scheduler.assign_machine(machine),
            _ => log::error!("Server not running, unable to assign machine."),
        }
    }
}


/// start the server
pub fn start_server() {
    // startup consists of several phases. The first is collecting assest from
    // the components. Then, using the assets, we start things up.
    log::info!("starting server");
    server_state.store(ServerState::Initializing);

    let monitor_factory = SystemMonitorFactory::new();
    let executor_factory = SystemExecutorFactory::new();
    let scheduler_factory = new_scheduler_factory();
    executor_factory.with_workers(get_executor_count());

    let executor =
        executor_factory.start(monitor_factory.get_sender(), scheduler_factory.get_sender());
    let monitor = monitor_factory.start(Arc::clone(&executor));
    let scheduler =
        scheduler_factory.start(monitor_factory.get_sender(), executor_factory.get_queues());

    let mut s = server.borrow_mut();
    s.monitor = ServerField::Monitor(monitor);
    s.scheduler = ServerField::Scheduler(scheduler);
    s.executor = ServerField::Executor(executor);
    server_state.store(ServerState::Running);
    log::info!("server is now running");
}

/// stop the server
pub fn stop_server() {
    log::info!("stopping server");
    server_state.store(ServerState::Stopping);

    if let ServerField::Executor(executor) = &server.borrow().executor { executor.stop() }
    if let ServerField::Scheduler(scheduler) = &server.borrow().scheduler { scheduler.stop() }
    if let ServerField::Monitor(monitor) = &server.borrow().monitor { monitor.stop() }

    let mut s = server.borrow_mut();
    s.scheduler = ServerField::Uninitialized;
    s.executor = ServerField::Uninitialized;
    s.monitor = ServerField::Uninitialized;

    server_state.store(ServerState::Stopped);
    log::info!("server is now stopped");
}

///
/// Control the number of executor threads. The default originates
/// here. It can be read and mutated, mutations after starting the
/// server have no effect.
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
pub static executor_count: AtomicCell<usize> = AtomicCell::new(4);
#[allow(dead_code)]
pub fn get_executor_count() -> usize {
    executor_count.load()
}
#[allow(dead_code)]
pub fn set_executor_count(new: usize) {
    executor_count.store(new);
}


#[cfg(test)]
pub mod tests {
    use super::*;
    use std::panic;

    // common function for wrapping a test with setup/teardown logic
    pub fn run_test<T>(test: T) -> ()
    where
        T: FnOnce() -> () + panic::UnwindSafe,
    {
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
            std::thread::sleep_ms(200);
        });
    }
}
