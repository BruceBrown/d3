use super::*;
use self::tls_executor::*;

///
/// A machine has both instruction set dependent and independent structure.
/// They are combined in the MachineAdapter. They are stored as a ShareableMachine,
/// which is just an Arc wrapper arount the MachineAdapter.
///

/// The MachineAdapter is the model for Machine in the Collective
#[derive(Debug)]
pub struct MachineAdapter {
    /// The id is assigned on creation, and is intended for to be used in logging
    id: Uuid,

    /// The key is assigned when the machine is assigned to the collective. When a
    /// machine is removed from the collective, it's key can be re-issued.
    pub key: usize,

    /// The state of the machine. Its an Arc<AtomicCell<CollectiveState>> allowing
    /// it to be shared with other adapters, particularly, the sender adapter when
    /// the Sender is parked.
    pub state: MachineState,

    /// The normalized machine adapter. Its wrapped in a Box, for sizing, and should
    /// be considered immutable. It is not shared with other adapters, however its
    /// contents may be shared.
    normalized_adapter: Box<dyn MachineDependentAdapter>,
}
impl MachineAdapter {
    #[inline]
    pub fn new(adapter: Box<dyn MachineDependentAdapter>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            key: 0,
            state: MachineState::default(),
            normalized_adapter: adapter
        }
    }
    #[inline]
    pub const fn get_id(&self) -> Uuid { self.id }
    #[inline]
    pub fn get_key(&self) -> usize { self.key }
    #[inline]
    pub fn get_state(&self) -> CollectiveState { self.state.get() }
    #[inline]
    pub fn set_state(&self, new: CollectiveState) { self.state.set(new); }
    #[inline]
    pub fn clone_state(&self) -> MachineState { self.state.clone() }
    // the remainder are implemented via a trait object
    #[inline]
    pub fn sel_recv<'a>(&'a self, sel: &mut crossbeam::Select<'a>) -> usize {
        self.normalized_adapter.sel_recv(sel)
    }
    #[inline]
    pub fn receive_cmd(&self, time_slice: Duration, stats: &mut ExecutorStats) {
        self.normalized_adapter.receive_cmd(&self.state, time_slice, stats)
    }
    #[inline]
    pub fn try_recv_task(&self, machine: &ShareableMachine) -> Option<Task> {
        self.normalized_adapter.try_recv_task(machine)
    }
}
// For fast exchange, a ShareableMachine is stored in the machine
// collective. It is cloned as a task, and moved into tls as a clone.
// The idea being that its faster to clone an Arc<> than it is to copy it.
pub type ShareableMachine = Arc<MachineAdapter>;


///
/// The state of the machine.
/// All machines start New.
/// A disconnected machine hasn't been told that its disconnected, onceit is its dead.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SmartDefault)]
#[allow(dead_code)]
pub enum CollectiveState {
    #[default]
    New,
    Waiting,
    Ready,
    Running,
    SendBlock,
    RecvBlock,
    Disconnected,
    Dead,
}

/// A thread-safe wrapped state, which can be cloned.
pub type MachineState = SharedProtectedObject<CollectiveState>;

///
/// The MachineDependentAdapter is an encapsulating trait. It encapsulates the
/// instruction set being used, otherwise a <T> would need to be exposed.
/// Exposing a <T> has ramification in schedulting and execution which
/// don't arise due to the encapsulation.
pub trait MachineDependentAdapter : Send + Sync + fmt::Debug {
    /// Prepare a select.recv()
    fn sel_recv<'a>(&'a self, sel: &mut crossbeam::Select<'a>) -> usize;
    /// Complete the select.recv() with a try_recv
    fn try_recv_task(&self, machine: &ShareableMachine) -> Option<Task>;
    /// Deliver the instruction into the machine.
    fn receive_cmd(&self, state: &MachineState, time_slice: Duration, stats: &mut ExecutorStats);
}