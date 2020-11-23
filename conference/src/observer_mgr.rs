/// Here we have an implementation of the observer pattern. It is designed around channels,
/// with notification implemented as sending a variant to one or more registered senders.
use super::*;

/// Subject is the trait which is implemented to manage observer registration and notification.
pub trait Subject {
    /// Register an Observer
    fn register_observer<T: MachineImpl + 'static>(&self, sender: Sender<T>);

    /// Unregister an Observer
    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>);

    /// Notify register Observers
    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T);
}

/// The ObserverMgr is a container of observers. It contains a map, where the key is a Enum and the value
/// is a set of registered senders for the enum. Notification is a matter of finding the correct set for a
/// variant parameter and sending it to all registered senders.
pub struct ObserverMgr {
    /// senders key is the typeid of T for a set of Sender<T>
    senders: RwLock<BTreeMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl ObserverMgr {
    /// create a new ObserverMgr
    pub fn new() -> Self {
        Self {
            senders: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Default for ObserverMgr {
    fn default() -> Self { Self::new() }
}

/// Implement the methods required of a subject
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
                Some(set) => {
                    set.insert(sender);
                },
                None => unreachable!(),
            },
            None => unreachable!(),
        }
    }

    fn unregister_observer<T: MachineImpl + 'static>(&self, sender: &Sender<T>) {
        let type_id = TypeId::of::<T>();
        let mut write = self.senders.write();
        if !write.contains_key(&type_id) {
            return;
        }
        match write.get_mut(&type_id) {
            Some(senders) => {
                if let Some(set) = senders.downcast_mut::<BTreeSet<Sender<T>>>() {
                    set.remove(sender);
                }
            },
            None => (),
        }
    }

    fn notify_observers<T: MachineImpl + Clone + MachineImpl<InstructionSet = T> + Debug>(&self, cmd: T) {
        let type_id = TypeId::of::<T>();
        let read = self.senders.read();
        if let Some(senders) = read.get(&type_id) {
            if let Some(set) = senders.downcast_ref::<BTreeSet<Sender<T>>>() {
                // could optimize for 1 or last 1, as you don't need to clone it
                for s in set {
                    send_cmd(s, cmd.clone())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::TryRecvError;
    use d3_derive::*;

    // cov: begin-ignore-line
    #[allow(dead_code)]
    #[derive(Debug, Copy, Clone, MachineImpl)]
    pub enum Calc {
        Clear,
    }
    // cov: end-ignore-line

    // cov: begin-ignore-line
    #[allow(dead_code)]
    #[derive(Debug, Copy, Clone, MachineImpl)]
    pub enum LightState {
        Red(bool),
    }
    // cov: end-ignore-line

    #[test]
    fn test_observer() {
        let mgr = ObserverMgr::default();
        let (alice_calc_sender, alice_calc_receiver) = channel::<Calc>();
        let (alice_light_sender, alice_light_receiver) = channel::<LightState>();
        let (bob_light_sender, bob_light_receiver) = channel::<LightState>();

        // make sure unregistering something not registered is harmless
        mgr.unregister_observer(&alice_light_sender);

        // now register senders
        mgr.register_observer(alice_calc_sender);
        mgr.register_observer(alice_light_sender.clone());
        mgr.register_observer(bob_light_sender);

        // send notification
        mgr.notify_observers(LightState::Red(true));

        // alice's calc shouldn't see it
        match alice_calc_receiver.try_recv() {
            Ok(_) => unreachable!(),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => unreachable!(),
        }

        // alice's light should see it
        match alice_light_receiver.try_recv() {
            Ok(LightState::Red(true)) => (),
            Ok(_) => unreachable!(),
            Err(_) => unreachable!(),
        }

        // bob's light should see it
        match bob_light_receiver.try_recv() {
            Ok(LightState::Red(true)) => (),
            Ok(_) => unreachable!(),
            Err(_) => unreachable!(),
        }

        // unregister alice from the light
        mgr.unregister_observer(&alice_light_sender);
        mgr.notify_observers(LightState::Red(true));

        // now alice shouldn't see it
        match alice_light_receiver.try_recv() {
            Ok(_) => unreachable!(),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => unreachable!(),
        }

        // bob should still see it
        match bob_light_receiver.try_recv() {
            Ok(LightState::Red(true)) => (),
            Ok(_) => unreachable!(),
            Err(_) => unreachable!(),
        }

        // alice should see the calc
        mgr.notify_observers(Calc::Clear);
        match alice_calc_receiver.try_recv() {
            Ok(Calc::Clear) => (),
            Err(_) => unreachable!(),
        }
    }
}
