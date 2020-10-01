#![allow(dead_code)]
use super::*;

#[derive(Debug, MachineImpl)]
pub enum EchoCmd {
    /// An instance making itself known
    /// (conn_id, instance's sender, instance flavor)
    Instance(u128, EchoCmdSender, settings::Component),

    /// An instance instructed to add a sink
    /// (conn_id, sink's sender)
    AddSink(u128, EchoCmdSender),
    
    /// An instance instructed that there's new data
    /// (conn_id, bytes)
    NewData(u128, EchoData),
}
pub type EchoCmdSender = Sender<EchoCmd>;
pub type EchoData = Arc<Vec<u8>>;
