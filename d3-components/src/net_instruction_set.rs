#![allow(dead_code)]
use super::*;
// ```sequence
// Alice->Net: BindListner(addres, Alice:sender)
// note right of Alice: Alice waits for a connection
// Net->Alice: NewConn(conn_id, local, remote, write_buf_size)
// note right of Alice: Alice is told of the connection
// and has Bob handle it
// Alice->Net: BindConn(conn_id, Bob:sender)
// note left of Bob: Bob waits for bytes
// Net->Bob: RecvBytes(conn_id, bytes)
// note left of Bob: Bob sends some bytes
// Bob->Net: SendBytes(conn_id, bytes)
// note left of Bob: Bob may be told how
// much more he can send
// Net->Bob: SendReady(write_buf_size)
// note left of Bob: Bob Closes the connection
// Bob->Net: Close(conn_id)
// ```

/// This is where d3 meets Tcp or UDP. The Network leans heavily upon Mio
/// to provide the foundation for interacting with the network. The d3 network provides
/// an additional abstraction to adapt it for d3 machines and encapsulate Mio.
///
/// This sequence diagram illustrates a machine, Alice asking for a TCP listener to
/// be installed. When there is a connection, the network is told to send control
/// and data commands to the machine Bob.
///
/// ```text
/// +-------+                                             +-----+                        +-----+
/// | Alice |                                             | Net |                        | Bob |
/// +-------+                                             +-----+                        +-----+
///    |                                                    |                              |
///    | BindListner(addres, Alice:sender)                  |                              |
///    |--------------------------------------------------->|                              |
///    | -------------------------------\                   |                              |
///    |-| Alice waits for a connection |                   |                              |
///    | |------------------------------|                   |                              |
///    |                                                    |                              |
///    |    NewConn(conn_id, local, remote, write_buf_size) |                              |
///    |<---------------------------------------------------|                              |
///    | ----------------------------------\                |                              |
///    |-| Alice is told of the connection |                |                              |
///    | | and has Bob handle it           |                |                              |
///    | |---------------------------------|                |                              |
///    | BindConn(conn_id, Bob:sender)                      |                              |
///    |--------------------------------------------------->|                              |
///    |                                                    |      ----------------------\ |
///    |                                                    |      | Bob waits for bytes |-|
///    |                                                    |      |---------------------| |
///    |                                                    |                              |
///    |                                                    | RecvBytes(conn_id, bytes)    |
///    |                                                    |----------------------------->|
///    |                                                    |     -----------------------\ |
///    |                                                    |     | Bob sends some bytes |-|
///    |                                                    |     |----------------------| |
///    |                                                    |                              |
///    |                                                    |    SendBytes(conn_id, bytes) |
///    |                                                    |<-----------------------------|
///    |                                                    |    ------------------------\ |
///    |                                                    |    | Bob may be told how   |-|
///    |                                                    |    | much more he can send | |
///    |                                                    |    |-----------------------| |
///    |                                                    | SendReady(write_buf_size)    |
///    |                                                    |----------------------------->|
///    |                                                    |----------------------------\ |
///    |                                                    || Bob Closes the connection |-|
///    |                                                    ||---------------------------| |
///    |                                                    |                              |
///    |                                                    |               Close(conn_id) |
///    |                                                    |<-----------------------------|
///    |                                                    |                              |
/// ```
///
/// UDP is a bit simpler, as illusted by the following sequence diagram. Alice asks the
/// network to bind a UDP address. Whenever a packet arrives it is sent to Alice. Alice
/// can send a packet to the network too. Although there isn't a connection, in the network
/// sense, a conn_id is used, for both parity and as a shorthand for the local address.
///
/// ```text
/// +-------+                                    +-----+
/// | Alice |                                    | Net |
/// +-------+                                    +-----+
///     |                                           |
///     | BindUdpListener(address, Alice:sender)    |
///     |------------------------------------------>|
///     | ------------------------------\           |
///     |-| Wait for a packet to arrive |           |
///     | |-----------------------------|           |
///     |                                           |
///     |    RecvPkt(conn_id, local, remote, bytes) |
///     |<------------------------------------------|
///     | -----------------------\                  |
///     |-| Alice sends a packet |                  |
///     | |----------------------|                  |
///     |                                           |
///     | SendPkt(conn_id, remote, bytes)           |
///     |------------------------------------------>|
///     |                                           |
/// ```
#[derive(Debug, MachineImpl)]
pub enum NetCmd {
    /// Stop the network. Stop is used in conjuntion with starting and stopping the
    /// Server and Network.
    Stop,
    /// Binds a TCP listener to an address, notifying the sender when a connection is accepted.
    BindListener(String, NetSender),
    /// Bind a UDP listener to an address, notifying the sender when a connection is accepted.
    BindUdpListener(String, NetSender),
    /// New connection notification (connection_id, bind_addr,
    /// connect_from, max_byte) are sent to the sender regitered via the BindListener.
    NewConn(NetConnId, String, String, usize),
    /// BindConn starts the flow of information (connection_id, sender) between the network and
    /// a sender.
    BindConn(NetConnId, NetSender),
    /// When sent to the network, CloseConn closes the connection, also known as a local close.
    /// When received by the BindConn listener, CloseConn is notification that the connection
    /// has been closed, also known as a remote close.
    CloseConn(NetConnId),
    /// Sent to the BindConn sender, RecvBytes provides bytes read from the connection.
    RecvBytes(NetConnId, Vec<u8>),
    /// Sent to UDP listener
    /// socket id, destination, source, bytes
    RecvPkt(NetConnId, String, String, Vec<u8>),
    /// Sent to the network, SendBytes provides bytes to be written to the network.
    SendBytes(NetConnId, Vec<u8>),
    /// Send UDP packet to network
    /// socket id, destination, bytes
    SendPkt(NetConnId, String, Vec<u8>),
    /// Sent to the BindConn sender, it provides an update to the number of bytes
    /// available for writing to the network. A use case for this would be providing
    /// feedback for throttling data being generated for a connection.
    SendReady(NetConnId, usize),
}
/// A network connection is always expressed as a NetConnId and identifies a specific
/// network connection.
pub type NetConnId = usize;
/// Shorthand for a sender, that can be sent NetCmd instructions.
pub type NetSender = Sender<NetCmd>;
