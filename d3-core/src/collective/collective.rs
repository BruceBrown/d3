use super::*;
///
/// There are some datastructure we need to deal with here. They
/// form the bridging between machines and the core. We define
/// an outer structure, which is machine and instruction set
/// independent. Within that there is the machine and instruction
/// set dependent parts, exposed as a trait object.
/// 
/// Let see how this all comes together. Starting with the build
/// of a machine.
/// 
/// A machine, implementing an instuction set is transforned into
/// an Arc<Mutex<T>>, where it is the machine. It also has
/// a Receiver<P>, which receives instructions from instruction
/// set P. Finally, when an instruction is received it is passed
/// to the machine. This is captured in the Adapter created by
/// `#[derive(MachineImpl)]`. The MachineDependentAdapter
/// is the normalized trait, the adapter itself is not exposed.
///
///
/*
pub type CommonCollectiveAdapter = Arc<dyn MachineDependentAdapter>;
#[derive(Clone)]
pub struct ShareableMachine {
    pub id: Uuid,
    pub key: usize,
    pub state: MachineState,
    // the normalized_adapter is an ugly trait object, which needs fixing
    pub normalized_adapter: CommonCollectiveAdapter,
}
impl ShareableMachine {
    pub const fn get_id(&self) -> Uuid { self.id }
    pub const fn get_key(&self) -> usize { self.key }
    pub fn get_state(&self) -> CollectiveState { self.state.get() }
    pub fn set_state(&self, new: CollectiveState) { self.state.set(new); }
    pub fn clone_state(&self) -> MachineState { self.state.clone() }
    // the remainder are implemented via a trait object
    pub fn sel_recv<'a>(&'a self, sel: &mut crossbeam::Select<'a>) -> usize {
        self.normalized_adapter.sel_recv(sel)
    }
    pub fn receive_cmd(&self, time_slice: Duration, stats: &mut ExecutorStats) {
        self.normalized_adapter.receive_cmd(&self.state, time_slice, stats)
    }
    pub fn try_recv_task(&self) -> Option<Task> {
        self.normalized_adapter.try_recv_task(self)
    }
}

///
/// The MachineDependentAdapter is an encapsulating trait. It encapsulates the
/// instruction set being used, otherwise a <T> would need to be exposed.
/// Exposing a <T> has ramification in schedulting and execution which
/// don't arise due to the encapsulation.
pub trait MachineDependentAdapter : Send + Sync {
    /// Prepare a select.recv()
    fn sel_recv<'a>(&'a self, sel: &mut crossbeam::Select<'a>) -> usize;
    /// Complete the select.recv() with a try_recv
    fn try_recv_task(&self, machine: &ShareableMachine) -> Option<Task>;
    /// Deliver the instruction into the machine.
    fn receive_cmd(&self, state: &MachineState, time_slice: Duration, stats: &mut ExecutorStats);
}
*/
/// static atomic cell w/128 isn't available, so use u64
#[allow(dead_code)]
/// Each new machine is assigned a unique id
pub static COLLECTIVE_ID: AtomicCell<u64> = AtomicCell::new(1);
/// The default channel queue size. It can be changed or overridden.
pub const CHANNEL_MAX: usize = 250;
/// The timeslice given to receive_cmd, allowing it to recv multiple commands
static TIMESLICE_IN_MILLIS: AtomicCell<usize> = AtomicCell::new(20);

pub fn get_time_slice() -> std::time::Duration { std::time::Duration::from_millis(TIMESLICE_IN_MILLIS.load() as u64)}
pub fn set_time_slice(new: std::time::Duration) { TIMESLICE_IN_MILLIS.store(new.as_millis() as usize) }
/// The task, created by the schedule and given to the executor.
/// @todo: consider dropping task and just using the raw ShareableMachine

/*
/// Executor statistics.
/// It lives here due to the ShareableMachine having it in a method signature
#[derive(Copy,Clone,Debug, Default, Eq, PartialEq)]
pub struct ExecutorStats {
    pub id: usize,
    pub tasks_executed: u128,
    pub instructs_sent: u128,
    pub blocked_senders: u128,
    pub max_blocked_senders: usize,
    pub exhausted_slice: u128,
    pub recv_time: std::time::Duration,
}
*/

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

}