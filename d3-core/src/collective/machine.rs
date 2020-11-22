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
    fn build_raw<T>(raw: T, channel_capacity: usize) -> (Arc<T>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send,
    {
        // need to review allocation strategy for bounded
        let (sender, receiver) = machine_impl::channel_with_capacity::<Self::InstructionSet>(channel_capacity);
        Self::build_common(raw, sender, receiver)
    }

    // add an instruction set
    fn build_addition<T>(machine: &Arc<T>, channel_capacity: usize) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
    {
        // need to review allocation strategy for bounded
        let (sender, receiver) = machine_impl::channel_with_capacity::<Self::InstructionSet>(channel_capacity);
        Self::build_addition_common(machine, sender, receiver)
    }

    // build with an unbounded queue capacity
    fn build_unbounded<T>(raw: T) -> (Arc<T>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send,
    {
        // need to review allocation strategy for bounded
        let (sender, receiver) = machine_impl::channel::<Self::InstructionSet>();
        Self::build_common(raw, sender, receiver)
    }

    // add an instruction set
    fn build_addition_unbounded<T>(machine: &Arc<T>) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send,
    {
        // need to review allocation strategy for bounded
        let (sender, receiver) = machine_impl::channel::<Self::InstructionSet>();
        Self::build_addition_common(machine, sender, receiver)
    }

    // common building, both build() and build_unbounded() should call into build_common()
    fn build_common<T>(
        raw: T, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>,
    ) -> (Arc<T>, Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
        <Self as MachineBuilder>::InstructionSet: Send,
    {
        let instance: Arc<T> = Arc::new(raw);
        let (sender, machine_adapter) = Self::build_addition_common(&instance, sender, receiver);
        (instance, sender, machine_adapter)
    }

    fn build_addition_common<T>(
        machine: &Arc<T>, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>,
    ) -> (Sender<Self::InstructionSet>, MachineAdapter)
    where
        T: 'static + Machine<Self::InstructionSet>,
    {
        // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
        let machine = Arc::clone(machine) as Arc<dyn Machine<Self::InstructionSet>>;
        // wrap the machine dependent bits into a trait object
        let machine_adapter = Self::build_adapter(machine, receiver);
        (sender, machine_adapter)
    }

    // Wrap the machine and receiver into a MachineAdapter trait object
    fn build_adapter(machine: Arc<dyn Machine<Self::InstructionSet>>, receiver: Receiver<Self::InstructionSet>) -> MachineAdapter;
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
    if let Err(e) = sender.send(cmd) {
        log::info!("failed to send instruction: {}", e);
    }
}
