#[allow(unused_imports)] use super::*;

use std::boxed::Box;
use std::io::{self, Read};
use std::net::SocketAddr;

use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};

use crossbeam_channel::TryRecvError;
use ringbuf::RingBuffer;
use slab::Slab;

// Here's how this all works... There's a single thread responsible the network. Essentially,
// there's a loop in which does the following:
//
//     Accept New Connections
//     Send Connection Bytes
//     Receive Connection Bytes
//     Read Channel Commands
//
// There are several competing goals:
//     This may be a long running server, with 1000s of connections a day.
//     There may be a large, but bounded(1024) number of bound listeners.
//     I thnk We want to have separate server and a connection token spaces.
//     Lookup should be fast.
//
// This turns into some decisions
//     Since token will need to be reused, we'll use a slab to maange things
//     We're going to use a cirular buffer for managing send data and provide
//     reports on available space.
//     We bounce between poll and channel reading and it needs to be responsive,
//     while also not driving CPU usage when things are idle. This sounds like
//     a job for a Waker and Select.
//
pub mod net {
    // this allows us to easily use ? for error handling
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
/// The network has some boilerplate which keeps the public exposure
/// small. It also allows swapping out the Factory and Instance as
/// they're exposes as trait objects.
///
/// This is the only public exposure, in here and it shouldn't leak out of the lib.
pub fn new_network_factory() -> NetworkFactoryObj {
    let (sender, receiver) = channel_with_capacity::<NetCmd>(250);
    Arc::new(SystemNetworkFactory { sender, receiver })
}

type NetReceiver = Receiver<NetCmd>;
// commonly use result_type

/// The scheduler factory, it is exposed as a trait object
struct SystemNetworkFactory {
    sender: NetSender,
    receiver: NetReceiver,
}
/// The implementation of the trait object for the factory
impl NetworkFactory for SystemNetworkFactory {
    /// get a clone of the sender for the schdeduler
    fn get_sender(&self) -> NetSender { self.sender.clone() }
    /// start the network
    fn start(&self) -> NetworkControlObj {
        log::info!("creating network");
        let res = Mio::new(self.sender.clone(), self.receiver.clone());
        Arc::new(res)
    }
}

struct Mio {
    sender: NetSender,
    thread: Option<thread::JoinHandle<()>>,
}
impl Mio {
    /// stop the network
    fn stop(&self) {
        log::info!("stopping network");
        send_cmd(&self.sender, NetCmd::Stop);
    }
    /// create the scheduler
    fn new(sender: NetSender, receiver: NetReceiver) -> Self {
        let thread = NetworkThread::spawn(receiver);
        Self { sender, thread }
    }
}

/// This is the trait object for Mio
impl NetworkControl for Mio {
    /// stop the scheduler
    fn stop(&self) { self.stop(); }
}

/// If we haven't done so already, attempt to stop the schduler thread
impl Drop for Mio {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            send_cmd(&self.sender, NetCmd::Stop);
            log::info!("synchronizing Network shutdown");
            if thread.join().is_err() {
                log::trace!("failed to join Network thread");
            }
        }
        log::info!("Network shutdown complete");
    }
}

/// tuning params
///
/// POLL_INTERVAL is how often poll will wake up, with a waker this can be long
const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// EVENT_CAPACITY is the number of simultaneous events that a single poll can return
const EVENT_CAPACITY: usize = 512;
/// MAX_SERVERS is a estimate of the number of server accept ports
const MAX_SERVERS: usize = 32;
/// MAX_BYTES is the maximum network read and write size
const MAX_BYTES: usize = 4096;
/// MAX_CONNECTIONS is the estimated number of simultaneous connections.
const MAX_CONNECTIONS: usize = 10000;

#[derive(Debug)]
struct Server {
    is_dead: bool,
    bind_addr: String,
    listener: TcpListener,
    sender: NetSender,
}

struct Connection {
    /// sender for connection events
    sender: NetSender,
    /// the tcp stream
    stream: TcpStream,
    /// is_Ready indicates that we can continue writing
    is_ready: bool,
    /// the send buffer, consumer is sending to network
    consumer: ringbuf::Consumer<u8>,
    /// the send buffer, producer is getting bytes from app
    producer: ringbuf::Producer<u8>,
    /// bytes written since last report
    send_count: usize,
}
impl Connection {
    // newly accept connection... fill in the bits.
    fn new(sender: NetSender, stream: TcpStream) -> Self {
        let (producer, consumer) = RingBuffer::<u8>::new(MAX_BYTES).split();
        Self {
            sender,
            stream,
            is_ready: false,
            consumer,
            producer,
            send_count: 0,
        }
    }
    // send bytes in the ring buffer, we'll allow a partial write
    fn send_bytes(&mut self, token: &Token) -> net::Result<()> {
        if self.consumer.is_empty() {
            return Ok(());
        }
        let result: net::Result<()> = loop {
            match self.consumer.write_into(&mut self.stream, None) {
                Ok(n) => {
                    self.send_count += n;
                    break Ok(());
                },
                Err(ref err) if would_block(err) => {
                    self.is_ready = false;
                    break Ok(());
                },
                Err(ref err) if interrupted(err) => (),
                Err(err) => break Err(Box::new(err)),
            }
        };
        // if we've sent 1/2 our capacity without a report, send a report
        if self.send_count > MAX_BYTES / 2 {
            self.send_count = 0;
            send_cmd(&self.sender, NetCmd::SendReady(token.0, self.producer.remaining()));
        }
        result
    }
}

struct NetworkWaker {
    waker: Arc<Waker>,
    mio_receiver: NetReceiver,
    waker_receiver: NetReceiver,
}
impl NetworkWaker {
    // the waker run loop
    fn run(&mut self) {
        // unfortuantely, we can't just wait for something to arrive, because we'll eat it.
        // So, setup a select for recv on the mio recevier and our receiver (for shutdown).
        let mut sel = crossbeam_channel::Select::new();
        sel.recv(&self.waker_receiver.receiver);
        sel.recv(&self.mio_receiver.receiver);
        log::info!("waker is starting");

        'outer: loop {
            let idx = sel.ready();
            match idx {
                0 => {
                    if self.check_yourself_before_you_wreck_yourself() {
                        break;
                    }
                },
                1 => loop {
                    // poke until done, cuz we might have hit an edge
                    if self.mio_receiver.receiver.is_empty() {
                        break;
                    }
                    self.waker.wake().expect("unable to wake");
                    // pause, so as to not bombard mio with events
                    std::thread::sleep(Duration::from_millis(10));
                    // we can get stuck in here during a shutdown
                    if self.check_yourself_before_you_wreck_yourself() {
                        break 'outer;
                    }
                },
                _ => (),
            };
        }
        log::info!("waker has stopped")
    }

    // return true to shutdown
    fn check_yourself_before_you_wreck_yourself(&self) -> bool {
        match self.waker_receiver.receiver.try_recv() {
            Ok(NetCmd::Stop) => true,
            Ok(_) => false,
            Err(TryRecvError::Disconnected) => true,
            Err(TryRecvError::Empty) => false,
        }
    }
}

const WAKE_TOKEN: Token = Token(1023);

struct NetworkThread {
    receiver: NetReceiver,
    waker_sender: NetSender,
    is_running: bool,
    poll: Poll,
    connections: Slab<Connection>,
    servers: Slab<Server>,
    waker_thread: Option<thread::JoinHandle<()>>,
}
impl NetworkThread {
    fn spawn(receiver: NetReceiver) -> Option<thread::JoinHandle<()>> {
        log::info!("Starting Network");
        let thread = std::thread::spawn(move || {
            let (waker_sender, waker_receiver) = channel::<NetCmd>();
            let mut net_thread = Self {
                receiver,
                waker_sender,
                is_running: true,
                poll: Poll::new().unwrap(),
                connections: Slab::with_capacity(MAX_CONNECTIONS),
                servers: Slab::with_capacity(MAX_SERVERS),
                waker_thread: None,
            };
            if net_thread.run(waker_receiver).is_err() {}
            if let Some(thread) = net_thread.waker_thread.take() {
                if thread.join().is_err() {
                    log::trace!("failed to join waker thread");
                }
            }
        });
        Some(thread)
    }

    fn run(&mut self, waker_receiver: NetReceiver) -> net::Result<()> {
        let mut events = Events::with_capacity(EVENT_CAPACITY);
        // get the waker running...all it does is wake poll so that it can service the channel
        let waker = Arc::new(Waker::new(self.poll.registry(), WAKE_TOKEN)?);
        {
            let waker = Arc::clone(&waker);
            let mio_receiver = self.receiver.clone();
            let thread = std::thread::spawn(move || {
                let mut net_waker = NetworkWaker {
                    waker,
                    mio_receiver,
                    waker_receiver,
                };
                net_waker.run();
            });
            self.waker_thread = Some(thread);
        }
        while self.is_running {
            // poll is a bit messy, it doesn't wake on event
            self.poll.poll(&mut events, Some(POLL_INTERVAL)).unwrap();
            for event in events.iter() {
                match event.token() {
                    token if token.0 == 1023 => (),
                    token if token.0 < 1023 => {
                        while let Some(server) = self.servers.get_mut(token.0) {
                            let (connection, address) = match server.listener.accept() {
                                Ok((connection, address)) => (connection, address),
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    break;
                                },
                                Err(_e) => {
                                    server.is_dead = true;
                                    break;
                                },
                            };
                            self.store_connection(&token, connection, address)?;
                        }
                    },
                    token => {
                        match self.handle_connection_event(&token, event) {
                            Ok(true) => match self.remove_connection(&token, true) {
                                Ok(_) => (),
                                Err(_e) => (), // remove_connection shouldn't fail
                            },
                            Err(_e) => (), // might want to log here
                            _ => (),
                        };
                    },
                }
            }
            // we've processed all of the events, now for the channel
            let _result = loop {
                let cmd = self.receiver.try_recv();
                // if let Ok(cmd) = &cmd { log::trace!("mio {:#?}", cmd) }
                let result = match cmd {
                    Ok(NetCmd::Stop) => {
                        self.is_running = false;
                        break Ok(());
                    },
                    Ok(NetCmd::BindListener(addr, sender)) => self.bind_listener(addr, sender),
                    Ok(NetCmd::BindConn(conn_id, sender)) => self.bind_connection(conn_id, sender),
                    Ok(NetCmd::CloseConn(conn_id)) => self.close_connection(conn_id),
                    Ok(NetCmd::SendBytes(conn_id, bytes)) => self.send_bytes(conn_id, bytes),

                    Ok(_) => {
                        log::warn!("unhandled NetCmd");
                        Ok(())
                    },
                    Err(TryRecvError::Disconnected) => {
                        self.is_running = false;
                        break Ok(());
                    },
                    Err(TryRecvError::Empty) => break Ok(()),
                };
                if result.is_err() {
                    break result;
                }
            };
        }
        // tell the waker that it's job is done
        send_cmd(&self.waker_sender, NetCmd::Stop);
        Ok(())
    }

    fn handle_connection_event(&mut self, token: &Token, event: &Event) -> net::Result<bool> {
        let key = token.0 - 1024;
        if let Some(conn) = self.connections.get_mut(key) {
            if event.is_writable() {
                conn.is_ready = true;
                conn.send_bytes(token)?;
            }

            if event.is_readable() {
                let mut connection_closed = false;
                let mut received_data = Vec::with_capacity(MAX_BYTES);
                // We can (maybe) read from the connection.
                loop {
                    let mut buf = [0; 256];
                    match conn.stream.read(&mut buf) {
                        Ok(0) => {
                            // Reading 0 bytes means the other side has closed the
                            // connection or is done writing, then so are we.
                            connection_closed = true;
                            break;
                        },
                        Ok(n) => received_data.extend_from_slice(&buf[.. n]),
                        // Would block "errors" are the OS's way of saying that the
                        // connection is not actually ready to perform this I/O operation.
                        Err(ref err) if would_block(err) => break,
                        Err(ref err) if interrupted(err) => continue,
                        // Other errors we'll consider fatal.
                        Err(err) => return Err(Box::new(err)),
                    }
                }
                if !received_data.is_empty() {
                    send_cmd(&conn.sender, NetCmd::RecvBytes(token.0, received_data));
                }
                if connection_closed {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn remove_connection(&mut self, token: &Token, notify: bool) -> net::Result<()> {
        let key = token.0 - 1024;
        let mut conn = self.connections.remove(key);
        if notify {
            send_cmd(&conn.sender, NetCmd::CloseConn(token.0));
        }
        self.poll.registry().deregister(&mut conn.stream)?;
        Ok(())
    }

    fn send_bytes(&mut self, conn_id: NetConnId, bytes: Vec<u8>) -> net::Result<()> {
        let key: usize = conn_id - 1024;
        let result = if let Some(conn) = self.connections.get_mut(key) {
            let _count = conn.producer.push_slice(bytes.as_slice());
            if conn.is_ready {
                conn.send_bytes(&Token(conn_id))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        result
    }

    fn close_connection(&mut self, conn_id: NetConnId) -> net::Result<()> {
        let token = Token(conn_id);
        self.remove_connection(&token, false)
    }

    fn bind_connection(&mut self, conn_id: NetConnId, sender: NetSender) -> net::Result<()> {
        let token = Token(conn_id);
        let key: usize = token.0 - 1024;
        if let Some(conn) = self.connections.get_mut(key) {
            conn.sender = sender;
            self.poll
                .registry()
                .register(&mut conn.stream, token, Interest::READABLE.add(Interest::WRITABLE))?;
        }
        Ok(())
    }

    fn bind_listener(&mut self, addr: String, sender: NetSender) -> net::Result<()> {
        let entry = self.servers.vacant_entry();
        let key = entry.key();
        let token = Token(key);
        let bind_addr = addr.parse().unwrap();
        let mut listener = TcpListener::bind(bind_addr)?;
        self.poll
            .registry()
            .register(&mut listener, token, Interest::READABLE)?;
        let server = Server {
            is_dead: false,
            bind_addr: addr,
            listener,
            sender,
        };
        entry.insert(server);
        Ok(())
    }

    // The server sender is stored as a placeholder, it will be replaced subsequently.
    fn store_connection(
        &mut self,
        server_token: &Token,
        connection: TcpStream,
        address: SocketAddr,
    ) -> net::Result<()> {
        if let Some(server) = self.servers.get(server_token.0) {
            let entry = self.connections.vacant_entry();
            let key = entry.key();
            let token = Token(key + 1024);
            let conn = Connection::new(server.sender.clone(), connection);
            entry.insert(conn);
            send_cmd(
                &server.sender,
                NetCmd::NewConn(token.0, server.bind_addr.to_string(), address.to_string(), MAX_BYTES),
            );
        }
        Ok(())
    }
}

fn would_block(err: &io::Error) -> bool { err.kind() == io::ErrorKind::WouldBlock }

fn interrupted(err: &io::Error) -> bool { err.kind() == io::ErrorKind::Interrupted }

#[cfg(test)]
mod tests {}
