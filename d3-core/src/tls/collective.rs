use self::tls_executor::*;
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// A machine has both instruction set dependent and independent structure.
// They are combined in the MachineAdapter. They are stored as a ShareableMachine,
// which is just an Arc wrapper arount the MachineAdapter.
//
// Every time a machine is added to the run_Q, a task is created. Each task carries
// an incremented task_id, as does the machine, 0 is a sentinal value. If the task_id
// doesn't match the machine task_id, there's been an unaccounted data race. Similarly,
// if when creating a task, the machine already has a task_id, there's been an
// unaccounted data-race. The machine's task_id is cleared at the end of its
// execution cycle.

// The MachineAdapter is the model for Machine in the Collective
#[doc(hidden)]
#[derive(Debug)]
pub struct MachineAdapter {
    // The id is assigned on creation, and is intended for to be used in logging
    id: Uuid,

    // The once flag, used for signalling connected.
    once: AtomicBool,

    // The disconnected flag, used for signalling disconnect from last sender.
    disconnected: AtomicBool,

    // The key is assigned when the machine is assigned to the collective. When a
    // machine is removed from the collective, it's key can be re-issued.
    pub key: AtomicUsize,

    // The state of the machine. Its an Arc<AtomicCell<MachineState>> allowing
    // it to be shared with other adapters, particularly, the sender adapter when
    // the Sender is parked.
    pub state: SharedMachineState,

    // Task id, this is 0 if not on a run_q, otherwise its the task id
    task_id: AtomicUsize,

    // The normalized machine adapter. Its wrapped in a Box, for sizing, and should
    // be considered immutable. It is not shared with other adapters, however its
    // contents may be shared.
    normalized_adapter: Box<dyn MachineDependentAdapter>,
}
impl MachineAdapter {
    #[inline]
    pub fn new(adapter: Box<dyn MachineDependentAdapter>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            once: AtomicBool::new(true),
            disconnected: AtomicBool::new(false),
            key: AtomicUsize::new(0),
            state: SharedMachineState::default(),
            task_id: AtomicUsize::new(0),
            normalized_adapter: adapter,
        }
    }
    #[inline]
    pub const fn get_id(&self) -> Uuid { self.id }
    #[inline]
    pub fn get_and_clear_once(&self) -> bool { self.once.swap(false, Ordering::SeqCst) }
    #[inline]
    pub fn get_key(&self) -> usize { self.key.load(Ordering::SeqCst) }
    #[inline]
    pub fn set_disconnected(&self) { self.disconnected.store(true, Ordering::SeqCst); }
    #[inline]
    pub fn is_disconnected(&self) -> bool { self.disconnected.load(Ordering::SeqCst) }
    #[inline]
    pub fn get_state(&self) -> MachineState { self.state.get() }
    #[inline]
    pub fn is_dead(&self) -> bool { self.state.get() == MachineState::Dead }
    #[inline]
    pub fn is_running(&self) -> bool { self.state.get() == MachineState::Running }
    #[inline]
    pub fn is_send_blocked(&self) -> bool { self.state.get() == MachineState::SendBlock }

    #[inline]
    pub fn compare_and_exchange_state(&self, current: MachineState, new: MachineState) -> Result<MachineState, MachineState> {
        self.state.compare_and_exchange(current, new)
    }
    pub fn clear_task_id(&self, stale: usize) {
        // log::trace!(
        // "clearing task {} stale {} for machine {} state {:#?}",
        // self.get_task_id(),
        // stale,
        // self.get_key(),
        // self.get_state()
        // );
        if stale != 0 && stale != self.get_task_id() {
            thread::sleep(Duration::from_millis(10));
            panic!("stale != current");
        }
        self.task_id.store(0, Ordering::SeqCst);
        // we already know that stale == taskid
        log::trace!("cleared task {} stale {} for machine {}", stale, stale, self.get_key());
    }
    pub fn get_task_id(&self) -> usize { self.task_id.load(Ordering::SeqCst) }
    pub fn set_task_id(&self, id: usize) { self.task_id.store(id, Ordering::SeqCst); }
    #[inline]
    pub fn is_channel_empty(&self) -> bool { self.normalized_adapter.is_channel_empty() }
    #[inline]
    pub fn channel_len(&self) -> usize { self.normalized_adapter.channel_len() }
    #[inline]
    pub fn set_state(&self, new: MachineState) { self.state.set(new); }
    #[inline]
    pub fn clone_state(&self) -> SharedMachineState { self.state.clone() }
    // the remainder are implemented via a trait object
    #[inline]
    pub fn receive_cmd(&self, machine: &ShareableMachine, drop: bool, time_slice: Duration, stats: &mut ExecutorStats) {
        self.normalized_adapter
            .receive_cmd(machine, self.get_and_clear_once(), drop, time_slice, stats)
    }
}
// For fast exchange, a ShareableMachine is stored in the machine
// collective. It is cloned as a task, and moved into tls as a clone.
// The idea being that its faster to clone an Arc<> than it is to copy it.
#[doc(hidden)]
pub type ShareableMachine = Arc<MachineAdapter>;
pub type WeakShareableMachine = Weak<MachineAdapter>;
// The state of the machine.
//
// All machines start New. When added to the collective, if their
// channel is empty, they are RecvBlock. Otherwise, they are
// Ready and a task is queued. When the task is picked up by
// an executor the state is Running. If found to be disconnected
// the state becomes Disconnected, once notified, the state
// becomes Dead. If, while running, a send blocks, the state
// becomes SendBlock. Once the send is allowed the state
// becomes Running, and the receiver's state if RecvBlock
// becomes Ready and a task is queued.

// After much futzing about, it is now clear that there are
// data-races happening between test/set of state and the
// sending of data and tasks. As a result, the state transitions
// will be pulled into a class and some synchronization will
// need to occur so that operations behave atomically. In particular,
// a guard will need to be acquired before state can be examined or
// updated, similarly, that guard will need to be acquired before
// send or recv on the machine's channel can occur.

#[doc(hidden)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, SmartDefault)]
#[allow(dead_code)]
pub enum MachineState {
    #[default]
    New,
    Waiting,
    Ready,
    Running,
    SendBlock,
    RecvBlock,
    // Disconnected,
    Dead,
}

// A thread-safe wrapped state, which can be cloned.
#[doc(hidden)]
pub type SharedMachineState = SharedProtectedObject<MachineState>;

// The MachineDependentAdapter is an encapsulating trait. It encapsulates the
// instruction set being used, otherwise a <T> would need to be exposed.
// Exposing a <T> has ramification in schedulting and execution which
// don't arise due to the encapsulation.
#[doc(hidden)]
pub trait MachineDependentAdapter: Send + Sync + fmt::Debug {
    // Deliver the instruction into the machine.
    fn receive_cmd(&self, machine: &ShareableMachine, once: bool, drop: bool, time_slice: Duration, stats: &mut ExecutorStats);
    // determine if channel is empty
    fn is_channel_empty(&self) -> bool;
    // get the number of elements in the channel
    fn channel_len(&self) -> usize;
}
