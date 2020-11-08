#[allow(unused_imports)] use super::*;

// The MachineBuilder lives in d3-collective. This is due to a
// dependency upon ShareableMachine and Sender.

// The MachineBuilder forms the interface between the machine and
// the collective. Each instruction set provides a means of
// tranforming a stand-alone machine into a collective, where it
// can interact with other machines.
//
// Building consists of wrapping a raw machine, so that it can be
// shared with the collective. A communications channel is added,
// which is provided for sending instructions into the machine,
// the receiver is owned by the collective.
//
// Although it may not be usual, a machine may support multiple
// instruction sets.
//
#[doc(hidden)]
pub trait MachineBuilder {
    // The instruction set implemented by the machine
    type InstructionSet: MachineImpl;

    // build with a fixed size queue capacity
    fn build_raw<T>(raw: T, channel_capacity: usize) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send;

    // add an instruction set
    fn build_addition<T>(machine: &Arc<Mutex<T>>, channel_capacity: usize) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>;

    // build with an unbounded queue capacity
    fn build_unbounded<T>(raw: T) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send;

    // add an instruction set
    fn build_addition_unbounded<T>(machine: &Arc<Mutex<T>>) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send;
    // common building, both build() and build_unbounded() should call into build_common()
    fn build_common<T>(
        raw: T, s: Sender<Self::InstructionSet>, r: Receiver<Self::InstructionSet>,
    ) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send;

    fn build_addition_common<T>(
        machine: &Arc<Mutex<T>>, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>,
    ) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>;
}

/// Send an instruction to a sender and log any errors. The instruction set
/// must implement Debug in order to use it in send_cmd()
///
/// # examples
///
/// ```
/// # use d3_core::machine_impl::*;
/// # use d3_derive::*;
/// # use std::sync::Arc;
/// # use parking_lot::Mutex;
/// # use d3_core::send_cmd;
/// # #[derive(Debug,MachineImpl)] pub enum StateTable { Start, Stop }
/// #
/// let (sender, receiver) = channel::<StateTable>();
/// send_cmd(&sender, StateTable::Start);
/// ```
#[inline]
pub fn send_cmd<T>(sender: &Sender<T>, cmd: T)
where
    T: MachineImpl + MachineImpl<InstructionSet = T> + std::fmt::Debug,
{
    match sender.send(cmd) {
        Ok(_) => (),
        Err(e) => log::info!("failed to send instruction: {}", e),
    }
}
