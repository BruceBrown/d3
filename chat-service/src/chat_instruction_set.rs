#![allow(dead_code)]
use super::*;

// We could just steal and modify Echo-Server, but its
// stable, so we will build out own and later merge them.

/// The instruction set for chatting
#[derive(Debug, MachineImpl)]
pub enum ChatCmd {
    /// An instance making itself known
    /// (conn_id, instance's sender, instance flavor)
    Instance(u128, ChatSender, settings::Component),
    /// An instance instructed to add a sink
    /// (conn_id, sink's sender)
    AddSink(u128, ChatSender),
    /// Remove a sink
    RemoveSink(u128),
    /// An instance instructed that there's new data
    /// (conn_id, bytes)
    NewData(u128, Data),
}
pub type ChatSender = Sender<ChatCmd>;
pub type Data = Arc<Vec<u8>>;
