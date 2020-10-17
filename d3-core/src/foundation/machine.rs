use super::*;

// Machines cooperate in a collective. There are several primary traits
// that are presented here. Combined, they form the contract between
// the machines, allowing them to interact with each other.
//
// The MachineImpl designates an instruction set, which machines implement.
// It carries with it, two adapters and an instruction set. The ReceiverAdapter
// is the primary means of expressing the machine. The SenderAdapter is
// ancilary, used for parking a sender, which would otherwise block the
// executor.
//
// To aid in construction, there's a derive macro, MachineImpl, which should
// be used to designate the instruction sets which machines implement.
// A machine may implement one or more of these.
//
// Examples
//
//     #[derive(Copy, Clone, Debug, Eq, MachineImpl, PartialEq, SmartDefault)]
//     #[allow(dead_code)]
//     pub enum ActivationCommands {
//         #[default]
//         Start,
//         Stop,
//     }
//

// Each enum, that is an instuction set, derives MachineImpl.
#[doc(hidden)]
pub trait MachineImpl: 'static + Send + Sync {
    type Adapter;
    type SenderAdapter;
    type InstructionSet: Send + Sync;

    // Park a sender. If the sender can't be parked, the instruction
    // is returned.
    fn park_sender(
        channel_id: usize,
        sender: crossbeam_channel::Sender<Self::InstructionSet>,
        instruction: Self::InstructionSet,
    ) -> Result<(), Self::InstructionSet>;
}

/// The machine is the common trait all machines must implement
/// and describes how instuctions are delivered to a machine, via
/// the receive method.
pub trait Machine<T>: Send + Sync
where
    T: 'static + Send + Sync,
{
    /// The receive method receives instructions sent to it by itself or other machines.
    fn receive(&self, cmd: T);
    /// The disconnected method is called to notify the machine that it has become disconnect and will no longer receive instructions.
    /// This could be a result of server shutdown, or all senders dropping their senders.
    fn disconnected(&self) {}
    /// The connected method is called once, before receive messages. It provides a notification that the
    /// machine has become connected and may receive instructions. It includes a Uuid for the machine,
    /// whic hmay be usedin logging. A machine implementing several instruction sets will receive a differnt
    /// Uuid for each instruction set.
    fn connected(&self, _uuid: Uuid) {}
}

// Adding the machine implementation to Mutex
// makes it possible to hide the underlying wrapping.
impl<T, P> Machine<P> for Mutex<T>
where
    T: Machine<P>,
    P: MachineImpl,
{
    fn receive(&self, cmd: P) { self.lock().unwrap().receive(cmd); }
    fn disconnected(&self) { self.lock().unwrap().disconnected(); }
    fn connected(&self, uuid: Uuid) { self.lock().unwrap().connected(uuid); }
}
