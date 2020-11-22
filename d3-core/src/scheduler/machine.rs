#![allow(dead_code)]
use super::*;

use self::setup_teardown::*;
use crate::channel::sender::*;

/// The connect method creates a machine, implementing an instruction set.
/// The machine has a bound communication channel of a default size receiving those instructions.
pub fn connect<T, P>(machine: T) -> (Arc<T>, Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>)
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let channel_max = default_channel_max.load();
    let (machine, mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_raw(machine, channel_max);
    bind_and_assign(collective_adapter, &mut sender);
    (machine, sender)
}

/// The and_connect method adds an additional instruction set and communication channel to the machine.
/// The communicate channel ib bound to a default size.
pub fn and_connect<T, P>(machine: &Arc<T>) -> Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let channel_max = default_channel_max.load();
    let (mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_addition(machine, channel_max);
    bind_and_assign(collective_adapter, &mut sender);
    sender
}

/// The connect_with_capacity method creates a machine with a bounded queue of the specified size.
pub fn connect_with_capacity<T, P>(
    machine: T, capacity: usize,
) -> (Arc<T>, Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>)
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let (machine, mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_raw(machine, capacity);
    bind_and_assign(collective_adapter, &mut sender);
    (machine, sender)
}

/// The and_connect_with_capacity method adds an additional instruction set and sender to the machine.
/// The communication channel is bound to the specified size.
pub fn and_connect_with_capacity<T, P>(
    machine: &Arc<T>, capacity: usize,
) -> Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let (mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_addition(machine, capacity);
    bind_and_assign(collective_adapter, &mut sender);
    sender
}

/// The connect_unbounded method creates a machine with an unbounded queue. It can result
/// in a panic if system resources become exhausted.
pub fn connect_unbounded<T, P>(machine: T) -> (Arc<T>, Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>)
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let (machine, mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_unbounded(machine);
    bind_and_assign(collective_adapter, &mut sender);
    (machine, sender)
}

/// The and_connect_unbounded method adds an additional instruction set and sender to the machine.
/// The communication channel is unbound.
pub fn and_connect_unbounded<T, P>(machine: &Arc<T>) -> Sender<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>
where
    T: 'static + Machine<P> + Machine<<<P as MachineImpl>::Adapter as MachineBuilder>::InstructionSet>,
    P: MachineImpl,
    <P as MachineImpl>::Adapter: MachineBuilder,
{
    let (mut sender, collective_adapter) = <<P as MachineImpl>::Adapter as MachineBuilder>::build_addition_unbounded(machine);
    bind_and_assign(collective_adapter, &mut sender);
    sender
}

// bind the adapter to the sender and assign the adapter to the collective
fn bind_and_assign<T>(adapter: MachineAdapter, sender: &mut Sender<T>)
where
    T: MachineImpl,
{
    let adapter = Arc::new(adapter);
    sender.bind(Arc::clone(&adapter));
    Server::assign_machine(adapter);
}

/// CHANNEL_MAX is the default size for bound communication channels.
pub const CHANNEL_MAX: usize = 250;

#[allow(dead_code)]
#[allow(non_upper_case_globals)]
/// The default_channel_max static is the default used for creating bound channels.
pub static default_channel_max: AtomicCell<usize> = AtomicCell::new(CHANNEL_MAX);
/// The get_default_channel_capacity function returns the default value.
#[allow(dead_code)]
pub fn get_default_channel_capacity() -> usize { default_channel_max.load() }
/// The set_default_channel_capacity function sets a new default value.
/// setting should be performed before starting the server.
#[allow(dead_code)]
pub fn set_default_channel_capacity(new: usize) { default_channel_max.store(new); }
