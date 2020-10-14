
#![allow(dead_code)]
use super::*;

///
/// This is where d3 meets Tcp (Udp to follow). The Network leans heavily upon Mio
/// to provide the foundation for interacting with the network. This is where a
/// further abstraction is applied to adapt it for d3 machines. This sequence diagram
/// illustrates that abstraction.
///
/// ``` WebSequence
/// Alice->Net: BindListner(addr, Alice:sender)
/// note right of Alice: Alice waits for a connection
/// Net->Alice: NewConn(conn_id, local_addr, remote_addr, write_buf_size)
/// note right of Alice: Alice is told of the connection and has Bob handle it
/// Alice->Net: BindConn(conn_id, Bob:sender)
/// note left of Bob: Bob gets some bytes
/// Net->Bob: RecvBytes(conn_id, bytes)
/// note left of Bob: Bob sends some bytes
/// Bob->Net: SendBytes(conn_id, bytes)
/// note left of Bob: Bob may be told how much more he can send
/// Net->Bob: SendReady(write_buf_size)
/// note left of Bob: Bob Closes the connection
/// Bob->Net: Close(conn_id)
/// ```
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
