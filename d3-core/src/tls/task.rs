// This may move...
use self::collective::*;
use self::scheduler::executor::{EXECUTORS_SNOOZING, RUN_QUEUE_LEN};
use self::scheduler::setup_teardown::Server;
use super::*;
use crossbeam::deque;

// Common types associated with a task
#[doc(hidden)]
pub type ExecutorTask = ShareableMachine;
pub type ExecutorInjector = Arc<deque::Injector<ExecutorTask>>;
pub type ExecutorWorker = deque::Worker<ExecutorTask>;
pub type ExecutorStealers = Vec<Arc<deque::Stealer<ExecutorTask>>>;

pub fn new_executor_injector() -> ExecutorInjector { Arc::new(deque::Injector::<ExecutorTask>::new()) }
pub fn new_executor_worker() -> ExecutorWorker { ExecutorWorker::new_fifo() }

pub static TASK_ID: AtomicUsize = AtomicUsize::new(1);

pub fn schedule_machine(machine: ShareableMachine, run_queue: &ExecutorInjector) { schedule_task(machine, run_queue) }
fn schedule_task(machine: ShareableMachine, run_queue: &ExecutorInjector) {
    RUN_QUEUE_LEN.fetch_add(1, Ordering::SeqCst);
    run_queue.push(machine);
    if EXECUTORS_SNOOZING.load(Ordering::SeqCst) != 0 {
        Server::wake_executor_threads();
    }
}

// generic function which finds a task
pub fn find_task<T>(local: &deque::Worker<T>, global: &deque::Injector<T>, stealers: &[Arc<deque::Stealer<T>>]) -> Option<T> {
    // Pop a task from the local queue, if not empty.
    local.pop().or_else(|| {
        // Otherwise, we need to look for a task elsewhere.
        iter::repeat_with(|| {
            // Try stealing a batch of tasks from the global queue.
            global
                .steal_batch_and_pop(local)
                // Or try stealing a task from one of the other executors.
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        // Loop while no task was stolen and any steal operation needs to be retried.
        .find(|s| !s.is_retry())
        // Extract the stolen task, if there is one.
        .and_then(|s| s.success())
    })
}
