#![allow(dead_code)]
use super::*;


/// This is the instruction set for network communication
#[derive(Debug, MachineImpl)]
pub enum NetCmd {
    /// Stop the network
    Stop,
    /// Bind listener to address, notify sender on connection
    BindListener(String, NetSender),
    /// New connection notification (connection_id, bind_addr, connect_from, max_byte )
    NewConn(u128, String, String, usize),
    /// BindConn starts the flow of data (connection_id, sender)
    BindConn(u128, NetSender),
    /// Close the connection
    CloseConn(u128),
    /// received (connection_id, bytes)
    RecvBytes(u128, Vec<u8>),
    /// push data out (connection, bytes)
    SendBytes(u128, Vec<u8>),
    /// ready to push more bytes out (connection, max_byte)
    SendReady(u128, usize),
}

/// SendReady may not be sent for every SendBytes. There's an attempt
/// to balance back and forth chatter with pipelining. Consequently, its
/// sent at pacing rate.
pub type NetSender = Sender<NetCmd>;
