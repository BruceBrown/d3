#![allow(dead_code)]
use super::*;

use crate::channel::{sender::*};
use self::setup_teardown::*;

///
/// connect, as the name implies, connects a machine into the system. Toss
/// in an object, implementing a Machine for an instruction set and get
/// back that machine, safely wrapped, along with a sender that you're
/// free to clone and hand out to other objects. There are 3 alternatives:
///     1. Create with a, default, fixed size queue.
///     2. Create with a fixed size queue, specified by the caller of connect.
///     3. Create with an unbound queue size.
///


///
/// connect creates a machine with a queue bound to a defauls size. When full,
/// the sender will block. However, it will not block the executor.
pub fn connect<T, P>(
    machine: T,
) -> (
    Arc<Mutex<T>>,
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
)
where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let channel_max = default_channel_max.load();
    let (machine, sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_raw(machine, channel_max);
    Server::assign_machine(collective_adapter);
    (machine, sender)
}

/// and_connect adds an additional instruction set and sender to the machine
pub fn and_connect<T, P>(
    machine: &Arc<Mutex<T>>,
) -> 
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>
where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let channel_max = default_channel_max.load();
    let (sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_addition(machine, channel_max);
    Server::assign_machine(collective_adapter);
    sender
}

///
/// connect_with_capacity creates a machine with a bounded queue. When full,
/// the sender will block. However, it will not block the executor.
pub fn connect_with_capacity<T, P>(
    machine: T,
    capacity: usize,
) -> (
    Arc<Mutex<T>>,
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
)
where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let (machine, sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_raw(machine, capacity);
    Server::assign_machine(collective_adapter);
    (machine, sender)
}

///
/// and_connect_with_capacity adds an additional instruction set and sender to the machine
pub fn and_connect_with_capacity<T, P>(
    machine: &Arc<Mutex<T>>,
    capacity: usize,
) ->
    
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>

where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let (sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_addition(machine, capacity);
    Server::assign_machine(collective_adapter);
    sender
}

///
/// connect_unbounded creates a machine with an unbounded queue. It can result
/// in a crash.
pub fn connect_unbounded<T, P>(
    machine: T,
) -> (
    Arc<Mutex<T>>,
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
)
where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let (machine, sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_unbounded(machine);
    Server::assign_machine(collective_adapter);
    (machine, sender)
}


///
/// and_connect_unbounded adds an additional instruction set and sender to the machine
pub fn and_connect_unbounded<T, P>(
    machine: &Arc<Mutex<T>>,
) ->
    Sender<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>

where
    T: 'static
        + Machine<P>
        + Machine<<<P as MachineImpl>::Adapter as MachineAdapter>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineAdapter,
{
    let (sender, collective_adapter) =
        <<P as MachineImpl>::Adapter as MachineAdapter>::build_addition_unbounded(machine);
    Server::assign_machine(collective_adapter);
    sender
}

/// This is where bounded channel defaulting is exposed. The default is picked
/// up and used here, where it can be read and mutated.
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
pub static default_channel_max: AtomicCell<usize> = AtomicCell::new(CHANNEL_MAX);
#[allow(dead_code)]
pub fn get_default_channel_capacity() -> usize {
    default_channel_max.load()
}
#[allow(dead_code)]
pub fn set_default_channel_capacity(new: usize) {
    default_channel_max.store(new);
}