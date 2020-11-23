use super::*;

/// Conferences are an organizational structure in which a conference models a container of
/// sessions. Sessions models a collection of machines carrying out a task for a specific entity
/// or user. At the top of this structure lives the One and Only ConferenceRoot.
///
/// In order to activate the conference structure, start_conferences() should be called as part
/// of server startup, and stop_conferences() should be called as part of server shutdown.

// The conference root knows about all conferences in the server

// cov: begin-ignore-line
#[derive(SmartDefault)]
enum ConferenceRootField {
    #[default]
    Uninitialized,
    ObserverMgr(ObserverMgr),
    Conferences(RwLock<BTreeMap<Uuid, Arc<Conference>>>),
}
// cov: end-ignore-line

#[allow(non_upper_case_globals)]
static conference_root: AtomicRefCell<ConferenceRoot> = AtomicRefCell::new(ConferenceRoot {
    observer_mgr: ConferenceRootField::Uninitialized,
    conferences: ConferenceRootField::Uninitialized,
});

// cov: begin-ignore-line
#[derive(SmartDefault)]
#[allow(dead_code)]
pub struct ConferenceRoot {
    observer_mgr: ConferenceRootField,
    conferences: ConferenceRootField,
}
// cov: end-ignore-line

impl ConferenceRoot {
    fn new() -> Self {
        let observer_mgr = ConferenceRootField::ObserverMgr(ObserverMgr::new());
        let conferences = ConferenceRootField::Conferences(RwLock::new(BTreeMap::new()));
        Self { observer_mgr, conferences }
    }
}

impl Subject for ConferenceRoot {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = self.observer_mgr {
            mgr.register_observer(sender)
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = self.observer_mgr {
            mgr.unregister_observer(sender)
        }
    }

    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = self.observer_mgr {
            mgr.notify_observers(cmd)
        }
    }
}

pub fn start_conferences() { *conference_root.borrow_mut() = ConferenceRoot::new(); }
pub fn stop_conferences() { start_conferences(); }

// The conference factory provide access to conferences
pub struct ConferenceFactory {}

impl ConferenceFactory {
    #[allow(dead_code)]
    fn join(conf_uuid: Uuid, sess_uuid: Uuid) -> Option<(Arc<Conference>, Arc<Session>)> {
        match conference_root.borrow().conferences {
            ConferenceRootField::Conferences(ref conf) => {
                {
                    let read = conf.read();
                    if read.contains_key(&conf_uuid) {
                        let res = match read.get(&conf_uuid) {
                            Some(conf) => Some((conf.clone(), conf.get_or_create_session(sess_uuid))),
                            None => unreachable!(),
                        };
                        return res;
                    }
                }
                let mut write = conf.write();
                if !write.contains_key(&conf_uuid) {
                    let new_conference = Arc::new(Conference::new(conf_uuid));
                    write.insert(conf_uuid, new_conference);
                }
                match write.get_mut(&conf_uuid) {
                    Some(conf) => Some((conf.clone(), conf.get_or_create_session(sess_uuid))),
                    None => unreachable!(),
                }
            },
            _ => unreachable!(),
        }
    }

    #[allow(dead_code, unused_variables)]
    fn leave(conference: Arc<Conference>, session: Arc<Session>) {}
}

/// Notifications across conferences.
impl Subject for ConferenceFactory {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = conference_root.borrow().observer_mgr {
            mgr.register_observer(sender)
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = conference_root.borrow().observer_mgr {
            mgr.unregister_observer(sender)
        }
    }

    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        if let ConferenceRootField::ObserverMgr(ref mgr) = conference_root.borrow().observer_mgr {
            mgr.notify_observers(cmd)
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
            uuid: Uuid::new_v4(),
            observer_mgr: ObserverMgr::new(),
            sessions: RwLock::new(BTreeMap::new()),
        }
    }
    fn get_or_create_session(&self, uuid: Uuid) -> Arc<Session> {
        {
            let read = self.sessions.read();
            if read.contains_key(&uuid) {
                if let Some(sess) = read.get(&uuid) {
                    return sess.clone();
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
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) { self.observer_mgr.register_observer(sender); }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) { self.observer_mgr.unregister_observer(sender); }

    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
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
            uuid: Uuid::new_v4(),
            observer_mgr: ObserverMgr::new(),
        }
    }
}

// Notifications between machines in a session
impl Subject for Session {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) { self.observer_mgr.register_observer(sender); }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) { self.observer_mgr.unregister_observer(sender); }

    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        self.observer_mgr.notify_observers(cmd);
    }
}

// cov: begin-ignore-line

#[cfg(test)]
mod tests {
    use super::*;
    use d3_derive::*;

        // cov: begin-ignore-line
        #[allow(dead_code)]
        #[derive(Debug, Copy, Clone, MachineImpl)]
        pub enum Calc {
            Clear,
        }
        // cov: end-ignore-line

    #[test]
    fn test_leave() {
        start_conferences();
        let conf_1_uuid = Uuid::new_v4();
        let sess_1_uuid = Uuid::new_v4();

        let (conf_1, sess_1) = ConferenceFactory::join(conf_1_uuid, sess_1_uuid).unwrap();
        ConferenceFactory::leave(conf_1, sess_1);
    }

    #[test]
    fn test_join() {
        start_conferences();
        let conf_1_uuid = Uuid::new_v4();
        let sess_1_uuid = Uuid::new_v4();
        let sess_2_uuid = Uuid::new_v4();

        let res = ConferenceFactory::join(conf_1_uuid, sess_1_uuid);
        assert_eq!(res.is_some(), true);
        let (conf_1, sess_1) = res.unwrap();
        assert_eq!(Arc::strong_count(&conf_1), 2);
        assert_eq!(Arc::strong_count(&sess_1), 2);

        let res = ConferenceFactory::join(conf_1_uuid, sess_2_uuid);
        let (conf_maybe_1, sess_2) = res.unwrap();
        assert_eq!(Arc::strong_count(&conf_maybe_1), 3);
        assert_eq!(Arc::strong_count(&sess_2), 2);

        let res = ConferenceFactory::join(conf_1_uuid, sess_1_uuid);
        assert_eq!(res.is_some(), true);
        let (conf_x, sess_x) = res.unwrap();
        assert_eq!(Arc::strong_count(&conf_1), 4);
        assert_eq!(Arc::strong_count(&sess_1), 3);
        assert_eq!(Arc::strong_count(&conf_x), 4);
        assert_eq!(Arc::strong_count(&sess_x), 3);
        stop_conferences();
    }

    #[test]
    fn test_conference_root_notifications() {

        start_conferences();
        let factory = ConferenceFactory{};

        let (alice_calc_sender, alice_calc_receiver) = channel::<Calc>();
        factory.register_observer(alice_calc_sender.clone());

        factory.notify_observers(Calc::Clear);
        // alice's calc should see it
        match alice_calc_receiver.try_recv() {
            Ok(Calc::Clear) => (),
            Err(_) => unreachable!(),
        }
        factory.unregister_observer(&alice_calc_sender);
     
        stop_conferences();
    }

    #[test]
    fn test_conference_notifications() {

        start_conferences();
        let conf_1_uuid = Uuid::new_v4();
        let sess_1_uuid = Uuid::new_v4();
        let (conf_1, _) = ConferenceFactory::join(conf_1_uuid, sess_1_uuid).unwrap();

        let (alice_calc_sender, alice_calc_receiver) = channel::<Calc>();
        conf_1.register_observer(alice_calc_sender.clone());

        conf_1.notify_observers(Calc::Clear);
        // alice's calc should see it
        match alice_calc_receiver.try_recv() {
            Ok(Calc::Clear) => (),
            Err(_) => unreachable!(),
        }
        conf_1.unregister_observer(&alice_calc_sender);
     
        stop_conferences();
    }

    #[test]
    fn test_conference_session() {

        start_conferences();
        let conf_1_uuid = Uuid::new_v4();
        let sess_1_uuid = Uuid::new_v4();
        let (_, sess_1) = ConferenceFactory::join(conf_1_uuid, sess_1_uuid).unwrap();

        let (alice_calc_sender, alice_calc_receiver) = channel::<Calc>();
        sess_1.register_observer(alice_calc_sender.clone());

        sess_1.notify_observers(Calc::Clear);
        // alice's calc should see it
        match alice_calc_receiver.try_recv() {
            Ok(Calc::Clear) => (),
            Err(_) => unreachable!(),
        }
        sess_1.unregister_observer(&alice_calc_sender);
     
        stop_conferences();
    }

}

// cov: end-ignore-line
