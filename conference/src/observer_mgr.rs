use super::*;

pub trait Subject {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>);
    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>);
    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T);
}

pub struct ObserverMgr {
    /// senders key is the typeid of T for a set of Sender<T>
    senders: RwLock<BTreeMap<TypeId, Box<dyn Any + Send + Sync>>>,
}
impl ObserverMgr {
    pub fn new() -> Self {
        Self {
            senders: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Subject for ObserverMgr {
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>) {
        let type_id = TypeId::of::<T>();
        let mut write = self.senders.write();
        if !write.contains_key(&type_id) {
            let new_senders = BTreeSet::<Sender<T>>::new();
            write.insert(type_id, Box::new(new_senders));
        }
        match write.get_mut(&type_id) {
            Some(senders) => match senders.downcast_mut::<BTreeSet<Sender<T>>>() {
                Some(set) => {set.insert(sender);},
                None => unreachable!(),
            },
            None => unreachable!(),
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        let type_id = TypeId::of::<T>();
        let mut write = self.senders.write();
        if !write.contains_key(&type_id) { return }
        match write.get_mut(&type_id) {
            Some(senders) => match senders.downcast_mut::<BTreeSet<Sender<T>>>() {
                Some(set) => {set.remove(&sender);},
                None => (),
            },
            None => unreachable!(),
        }
    }

    fn notify_observers<T:MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        let type_id = TypeId::of::<T>();
        let read = self.senders.read();
        match read.get(&type_id) {
            Some(senders) => match senders.downcast_ref::<BTreeSet<Sender<T>>>() {
                Some(set) => for s in set { send_cmd(&s, cmd.clone()) },
                None => (),
            }
            None => (),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::TryRecvError;
    use d3_derive::*;

#[derive(Debug, Copy, Clone, MachineImpl)]
pub enum Calc {
    Clear,
    Add(i32),
    Sub(i32),
}


#[derive(Debug, Copy, Clone, MachineImpl)]
pub enum LightState {
    Red(bool),
    Yellow(bool),
    Green(bool)
}

    #[test]
    fn test_observer () {
        let mut mgr = ObserverMgr::new();
        let (alice_calc_sender, alice_calc_receiver) = channel::<Calc>();
        let (alice_light_sender, alice_light_receiver) = channel::<LightState>();
        let (bob_light_sender, bob_light_receiver) = channel::<LightState>();

        mgr.register_observer(alice_calc_sender);
        mgr.register_observer(alice_light_sender.clone());
        mgr.register_observer(bob_light_sender);

        mgr.notify_observers(LightState::Red(true));

        match alice_calc_receiver.try_recv() {
            Ok(_) => assert_eq!(true, false),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => assert_eq!(true,false),
        }
        match alice_light_receiver.try_recv() {
            Ok(LightState::Red(true)) => (),
            Ok(_) => assert_eq!(true, false),
            Err(_) => assert_eq!(true,false),
        }
        match bob_light_receiver.try_recv() {
            Ok(LightState::Red(true)) => (),
            Ok(_) => assert_eq!(true, false),
            Err(_) => assert_eq!(true,false),
        }

        mgr.unregister_observer(&alice_light_sender);
        mgr.notify_observers(LightState::Red(true));
        match alice_light_receiver.try_recv() {
            Ok(_) => assert_eq!(true, false),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => assert_eq!(true,false),
        }
    }
}