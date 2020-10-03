use super::*;

/// The Connection provides a weak refernce between the sender and receiver.
/// The value of which is dubious. It may provide some feedback mechanisms.
/// It may also provide some insights into a live system to graph the data
/// flow.
#[derive(Default)]
pub struct Connection {
    connection: Weak<Mutex<Connection>>,
}

impl Connection {
    pub fn new() -> (Arc<Mutex<Self>>, Arc<Mutex<Self>>) {
        let c1 = Arc::new(Mutex::new(Self::default()));
        let c2 = Arc::new(Mutex::new(Self::default()));
        c1.lock().unwrap().connection = Arc::downgrade(&c2);
        c2.lock().unwrap().connection = Arc::downgrade(&c1);
        (c1, c2)
    }
}

pub type ThreadSafeConnection = Arc<Mutex<Connection>>;
