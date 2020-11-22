use super::*;

///
/// Conferences are an organizational structure in which a conference models a container of 
/// sessions. Sessions models a collection of machines carrying out a task for a specific entity
/// or user. At the top of this structure lives the One and Only ConferenceRoot.
/// 
/// In order to activate the conference structure, start_conferences() should be called as part
/// of server startup, and stop_conferences() should be called as part of server shutdown.

// The conference root knows about all conferences in the server

#[derive(SmartDefault)]
enum ConferenceRootField {
    #[default]
    Uninitialized,
    ObserverMgr(ObserverMgr),
    Conferences(RwLock<BTreeMap<Uuid, Arc<Conference>>>),
}

#[allow(non_upper_case_globals)]
static conference_root: AtomicRefCell<ConferenceRoot> = AtomicRefCell::new(ConferenceRoot{
    observer_mgr: ConferenceRootField::Uninitialized,
    conferences: ConferenceRootField::Uninitialized,
});

#[derive(SmartDefault)]
#[allow(dead_code)]
pub struct ConferenceRoot {
    observer_mgr: ConferenceRootField,
    conferences: ConferenceRootField,
}
impl ConferenceRoot {
    fn new() -> Self {
        let observer_mgr = ConferenceRootField::ObserverMgr(ObserverMgr::new());
        let conferences = ConferenceRootField::Conferences(RwLock::new(BTreeMap::new()));
        Self {observer_mgr, conferences}
    }
}

impl Subject for ConferenceRoot {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        match self.observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.register_observer(sender),
            _ => (),
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        match self.observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.unregister_observer(&sender),
            _ => (),
        }
    }

    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        match self.observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.notify_observers(cmd),
            _ => (),
        }
    }
}


pub fn start_conferences() {
    *conference_root.borrow_mut() = ConferenceRoot::new();
}
pub fn stop_conferences() {
    start_conferences();
}

// The conference factory provide access to conferences
pub struct ConferenceFactory {}


impl ConferenceFactory {
    #[allow(dead_code)]
    fn join(conf_uuid: Uuid, sess_uuid: Uuid) ->Option<(Arc<Conference>, Arc<Session>)> {
        match conference_root.borrow().conferences {
            ConferenceRootField::Conferences(ref conf) => {
                {
                    let read = conf.read();
                    if read.contains_key(&conf_uuid) {
                        let res = match read.get(&conf_uuid) {
                            Some(conf) => Some((conf.clone(), conf.get_or_create_session(sess_uuid))),
                            None => None
                        };
                        return res
                    }
                }
                let mut write = conf.write();
                if !write.contains_key(&conf_uuid) {
                    let new_conference = Arc::new(Conference::new(conf_uuid));
                    write.insert(conf_uuid, new_conference);
                }
                match write.get_mut(&conf_uuid) {
                    Some(conf) => Some((conf.clone(), conf.get_or_create_session(sess_uuid))),
                    None => None,
                }
            }
            _ => None,
        }
    }
    #[allow(dead_code, unused_variables)]
    fn leave(conference: Arc<Conference>, session: Arc<Session>) {

    }
}

/// Notifications across conferences.
impl Subject for ConferenceFactory {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        match conference_root.borrow().observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.register_observer(sender),
            _ => (),
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        match conference_root.borrow().observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.unregister_observer(&sender),
            _ => (),
        }
    }

    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        match conference_root.borrow().observer_mgr {
            ConferenceRootField::ObserverMgr(ref mgr) => mgr.notify_observers(cmd),
            _ => (),
        }
    }
}

#[allow(dead_code)]
pub struct Conference {
    uuid: Uuid,
    observer_mgr: ObserverMgr,
    sessions: RwLock<BTreeMap<Uuid, Arc<Session>>>,
}
impl Conference {
    #[allow(dead_code, unused_variables)]
    fn new(uuid: Uuid) -> Self {
        Self {
            uuid: Uuid::default(),
            observer_mgr: ObserverMgr::new(),
            sessions: RwLock::new(BTreeMap::new()),
        }
    }
    fn get_or_create_session(&self, uuid: Uuid) -> Arc<Session> {
        {
            let read = self.sessions.read();
            if read.contains_key(&uuid) {
                match read.get(&uuid) {
                    Some(sess) => return sess.clone(),
                    None => (),
                }
            }
        }
        let mut write = self.sessions.write();
        if !write.contains_key(&uuid) {
            let new_session = Arc::new(Session::new(uuid));
            write.insert(uuid, new_session);
        }
        match write.get(&uuid) {
            Some(sess) => sess.clone(),
            None => unreachable!(),
        }
    }
}

// Notifications between sessions in a conference
impl Subject for Conference {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        self.observer_mgr.register_observer(sender);
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        self.observer_mgr.unregister_observer(&sender);
    }

    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        self.observer_mgr.notify_observers(cmd);
    }
}

#[allow(dead_code, unused_variables)]
struct Session {
    uuid: Uuid,
    observer_mgr: ObserverMgr,
}
impl Session {
    #[allow(dead_code, unused_variables)]
    fn new(uuid: Uuid) -> Self {
        Self {
            uuid: Uuid::default(),
            observer_mgr: ObserverMgr::new(),
        }
    }
}

// Notifications between machines in a session
impl Subject for Session {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        self.observer_mgr.register_observer(sender);
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        self.observer_mgr.unregister_observer(&sender);
    }

    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        self.observer_mgr.notify_observers(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conference() {
        start_conferences();
        let conf_1_uuid = Uuid::default();
        let sess_1_uuid = Uuid::default();
        let sess_2_uuid = Uuid::default();

        let res = ConferenceFactory::join(conf_1_uuid, sess_1_uuid);
        assert_eq!(res.is_some(), true);
        let (conf_1, sess_1) = res.unwrap();
        let res = ConferenceFactory::join(conf_1_uuid, sess_2_uuid);
        let (conf_maybe_1, sess_2) = res.unwrap();
        assert_eq!(conf_1, conf_maybe_1);
        assert_ne!(sess_1, sess_2);
    }
}