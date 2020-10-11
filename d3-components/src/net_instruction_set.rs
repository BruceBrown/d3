#![allow(dead_code)]
use super::*;

/// This is the instruction set for network communication. A few things to note:
/// The connection id is usize, however components will want to translate that into
/// a uuid for the purposes of logging.
///
/// SendReady may not be sent for every SendBytes. There's an attempt
/// to balance back and forth chatter with pipelining. Consequently, its
/// sent at a pacing rate.

#[derive(Debug, MachineImpl)]
pub enum NetCmd {
    /// Stop the network
    Stop,
    /// Binds a TCP listener to an address, notifying the sender when a connection is accepted.
    BindListener(String, NetSender),
    /// New connection notification (connection_id, bind_addr,
    /// connect_from, max_byte) are sent to the sender regitered via the BindListener.
    NewConn(NetConnId, String, String, usize),
    /// BindConn starts the flow of data (connection_id, sender) between the network and
    /// a sender.
    BindConn(NetConnId, NetSender),
    /// Closes the connection, or provides notification to the BindConn sender that
    /// the connection was remotely closed.
    CloseConn(NetConnId),
    /// Provides bytes read from the network to the BindConn sender (connection_id, bytes).
    RecvBytes(NetConnId, Vec<u8>),
    /// Provides bytes to be written to the network.
    SendBytes(NetConnId, Vec<u8>),
    /// A notification that additional bytes can be written to the network. (connection, max_byte).
    SendReady(NetConnId, usize),
}
pub type NetConnId = usize;
pub type NetSender = Sender<NetCmd>;
