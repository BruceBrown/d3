extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse_macro_input;
use syn::DeriveInput;

/// #[derive(MachineImpl)]
///
/// Add (MachineImpl) to and enum's derive list to generate the code
/// necessary for transforming that enum into a set of directives for
/// a machine. The resulting code implements a MachineImpl for that
/// instruction set, along with several adapters to form the basis
/// of a machine which can interact with other maachines.
///
#[proc_macro_derive(MachineImpl)]
pub fn derive_machine_impl_fn(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let adapter_ident = format_ident!("MachineBuilder{}", name.to_string());
    let sender_adapter_ident = format_ident!("SenderAdapter{}", name.to_string());
    //let recv_wait_ident = format_ident!("RecvWait{}", name.to_string());
    let expanded = quote! {
        impl MachineImpl for #name {
            type Adapter = #adapter_ident;
            type SenderAdapter = #sender_adapter_ident;
            type InstructionSet = #name;
            fn park_sender(channel_id: usize, sender: crossbeam::Sender<Self::InstructionSet>, instruction: Self::InstructionSet) -> Result<(),Self::InstructionSet> {
                //Err(instruction)
                tls_executor_data.with(|t|{
                    let mut tls = t.borrow_mut();
                    // if its the main thread, let it block.
                    if tls.id == 0 { Err(instruction) }
                    else {
                        if let ExecutorDataField::Machine(machine) = &tls.machine {
                           let adapter = #sender_adapter_ident {
                                id: machine.get_id(),
                                key: machine.get_key(),
                                state: machine.state.clone(),
                                sender: sender,
                                instruction: Some(instruction),
                            };
                            let shared_adapter = SharedCollectiveSenderAdapter {
                                id: adapter.id,
                                key: adapter.key,
                                state: adapter.state.clone(),
                                normalized_adapter: Box::new(adapter),
                            };
                            tls.sender_blocked(channel_id, shared_adapter);
                        }
                        Ok(())
                    }
                })
            }
        }

        // This is the instruction set dependent machine, we've forgone generic <T>
        // as it becomes unwieldy when it comes to scheduling and execution. For the
        // most part it is immutable, with the only exception being the instruction,
        // which unfortunately has to travel between threads. We don't want to recreate
        // this for each instruction sent, so we're going to wrap the instruction
        // with Arc<Mutex> to allow inner access, kinda sucks cuz the lifecycle is
        // such that there is only one owner at a time. Let's see if Arc<> is good enough
        pub struct #adapter_ident {
            pub machine: Arc<dyn Machine<#name>>,
            pub receiver: Receiver<#name>,
            pub instruction: crossbeam::atomic::AtomicCell<Option<#name>>,
        }
        impl #adapter_ident {
            fn set_instruction(&self, inst: Option<#name>) {
                self.instruction.store(inst);
            }
            fn take_instruction(&self) -> Option<#name> {
                self.instruction.take()
            }
        }
        impl std::fmt::Debug for #adapter_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "#adapter_ident {{ .. }}")
            }
        }
        // This is the generic adapter implementation for the adapter, much of this is
        // already generic, so maybe there's an alternative where the dyn stuff can
        // be used less often.
        //
        impl MachineDependentAdapter for #adapter_ident {
            fn sel_recv<'b>(&'b self, sel: &mut crossbeam::Select<'b>) -> usize {
                sel.recv(&self.receiver.receiver)
            }
            fn try_recv_task(&self, machine: &ShareableMachine) -> Option<Task> {
                match self.receiver.receiver.try_recv() {
                    Err(crossbeam::TryRecvError::Empty) => None,
                    Err(crossbeam::TryRecvError::Disconnected) => {
                        machine.state.set(CollectiveState::Disconnected);
                        let task_adapter = Arc::clone(machine);
                        let task = Task{start: std::time::Instant::now(), machine: task_adapter };
                        Some(task)
                    }
                    Ok(instruction) => {
                        machine.state.set(CollectiveState::Ready);
                        let task_adapter = Arc::clone(machine);
                        self.set_instruction(Some(instruction));
                        let task = Task{start: std::time::Instant::now(), machine: task_adapter };
                        Some(task)
                    }
                }
            }
            fn receive_cmd(&self, state: &MachineState, once: bool, time_slice: std::time::Duration, stats: &mut ExecutorStats) {
                if once {
                    self.machine.connected();
                }
                if state.get() == CollectiveState::Disconnected {
                    state.set(CollectiveState::Running);
                    self.machine.disconnected();
                    state.set(CollectiveState::Dead);
                    return
                }
                state.set(CollectiveState::Running);
                // while we're running, might as well try to drain the queue, but keep it bounded
                let start = std::time::Instant::now();
                let mut cmd = self.take_instruction().unwrap();
                stats.tasks_executed += 1;
                loop {
                    self.machine.receive(cmd);
                    stats.instructs_sent += 1;
                    if start.elapsed() > time_slice {
                        stats.exhausted_slice += 1;
                        break
                    }
                    if state.get() != CollectiveState::Running { break }
                    match self.receiver.receiver.try_recv() {
                        Ok(m) => cmd = m,
                        Err(crossbeam::TryRecvError::Empty) => break,
                        Err(crossbeam::TryRecvError::Disconnected) => {
                            self.machine.disconnected();
                            state.set(CollectiveState::Dead);
                            break
                        }
                    };
                }
            }
        }

        pub struct #sender_adapter_ident {
            pub id: uuid::Uuid,
            pub key: usize,
            pub state: MachineState,
            pub sender: crossbeam::Sender<#name>,
            pub instruction: Option<#name>,
        }
        impl #sender_adapter_ident {
            fn try_send(&mut self) -> Result<(), TrySendError> {
                let instruction = self.instruction.take().unwrap();
                match self.sender.try_send(instruction) {
                    Ok(()) => {
                        self.state.set(CollectiveState::Running);
                        Ok(())
                    },
                    Err(crossbeam::TrySendError::Disconnected(inst)) => {
                        self.state.set(CollectiveState::Running);
                        self.instruction = Some(inst);
                        Err(TrySendError::Disconnected)
                    },
                    Err(crossbeam::TrySendError::Full(inst)) => {
                        self.instruction = Some(inst);
                        Err(TrySendError::Full)
                    },
                }
            }
        }

        impl CollectiveSenderAdapter for #sender_adapter_ident {
            fn get_id(&self) -> uuid::Uuid { self.id }
            fn get_key(&self) -> usize { self.key }
            fn try_send(&mut self) -> Result<(), TrySendError> {
                match self.try_send() {
                    Err(e) => Err(e),
                    Ok(()) => {
                        self.state.set(CollectiveState::Running);
                        Ok(())
                   },
                }
            }
        }

        impl MachineBuilder for #adapter_ident {
            type InstructionSet = #name;

            /// Consume a raw machine, using it to create a machine that is usable by
            /// the framework.
            fn build_raw<T>(raw: T, channel_capacity: usize) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition<T>(machine: &Arc<Mutex<T>>, channel_capacity: usize) -> (Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_unbounded<T>(raw: T) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition_unbounded<T>(machine: &Arc<Mutex<T>>) -> (Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_common<T>(raw: T, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // wrap it
                 let instance: Arc<Mutex<T>> = Arc::new(Mutex::new(raw));
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = Arc::clone(&instance) as Arc<dyn Machine<Self::InstructionSet>>;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                     instruction: crossbeam::atomic::AtomicCell::new(None),
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let machine_adapter = MachineAdapter::new(Box::new(adapter));
                 (instance, sender, machine_adapter)
            }

            fn build_addition_common<T>(machine: &Arc<Mutex<T>>, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (Sender<Self::InstructionSet>, MachineAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = Arc::clone(machine) as Arc<dyn Machine<Self::InstructionSet>>;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                     instruction: crossbeam::atomic::AtomicCell::new(None),
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let machine_adapter = MachineAdapter::new(Box::new(adapter));
                 (sender, machine_adapter)
            }
        }

        // build constructs a instruction specific machine. It is converted
        // from that into a trait object that can be shared via From. Doing
        // it here, rather than in build allows for some changes in design
        // down the road.
        /*
        impl From<#adapter_ident> for CommonCollectiveAdapter {
            fn from(adapter: #adapter_ident) -> Self {
                std::sync::Arc::new(adapter) as CommonCollectiveAdapter
            }
        }
        */
    };
    TokenStream::from(expanded)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
