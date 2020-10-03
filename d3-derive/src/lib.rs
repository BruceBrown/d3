extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse_macro_input;
use syn::DeriveInput;

/// #[derive(MachineImpl)]
///
/// Tag an Enum that is it is an instruction set for machines. This ends
/// up implementing a MachineImpl trait on the enum and a MachineAdapter
/// trait on the ReceiverAdapater, which is the template for all machines.
///
#[proc_macro_derive(MachineImpl)]
pub fn derive_machine_impl_fn(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let adapter_ident = format_ident!("MachineAdapter{}", name.to_string());
    let sender_adapter_ident = format_ident!("SenderAdapter{}", name.to_string());
    //let recv_wait_ident = format_ident!("RecvWait{}", name.to_string());
    let expanded = quote! {
        impl MachineImpl for #name {
            type Adapter = #adapter_ident;
            type SenderAdapter = #sender_adapter_ident;
            type InstructionSet = #name;
            fn block_or_continue() {
                tls_executor_data.with(|t|{
                    let mut tls = t.borrow_mut();
                    // main thread can always continue and block
                    if tls.id == 0 { return }
                    if tls.machine_state.get() != CollectiveState::Running {
                        tls.recursive_block();
                    }
                });
            }
            fn park_sender(channel_id: usize, sender: crossbeam::Sender<Self::InstructionSet>, instruction: Self::InstructionSet) -> Result<(),Self::InstructionSet> {
                //Err(instruction)
                tls_executor_data.with(|t|{
                    let mut tls = t.borrow_mut();
                    // if its the main thread, let it block.
                    if tls.id == 0 { Err(instruction) }
                    else {
                        let adapter = #sender_adapter_ident {
                            id: tls.machine_id,
                            state: tls.machine_state.clone(),
                            sender: sender,
                            instruction: Some(instruction),
                        };
                        let shared_adapter = SharedCollectiveSenderAdapter {
                            id: tls.machine_id,
                            state: tls.machine_state.clone(),
                            normalized_adapter: Box::new(adapter),
                        };
                        tls.sender_blocked(channel_id, shared_adapter);
                        Ok(())
                    }
                })
            }
        }

        // This is the instruction set dependent machine, we've forgone generic <T>
        // as it becomes unwieldy when it comes to scheduling and execution.
        pub struct #adapter_ident {
            pub machine: Arc<dyn Machine<#name>>,
            pub receiver: Receiver<#name>,
            pub instruction: Option<#name>,
        }

        // This is the generic adapter implementation for the adapter, much of this is
        // already generic, so maybe there's an alternative where the dyn stuff can
        // be used less often.
        //
        impl CollectiveAdapter for #adapter_ident {
            fn sel_recv<'b>(&'b self, sel: &mut crossbeam::Select<'b>) -> usize {
                sel.recv(&self.receiver.receiver)
            }
            fn try_recv_task(&self, machine: &SharedCollectiveAdapter) -> Option<Task> {
                match self.receiver.receiver.try_recv() {
                    Err(crossbeam::TryRecvError::Empty) => None,
                    Err(crossbeam::TryRecvError::Disconnected) => {
                        machine.state.set(CollectiveState::Disconnected);
                        let adapter = Self {
                            machine: self.machine.clone(),
                            receiver: self.receiver.clone(),
                            instruction: None,
                        };
                        let task_adapter = SharedCollectiveAdapter {
                            id: machine.id,
                            state: machine.state.clone(),
                            normalized_adapter: Box::new(adapter) as CommonCollectiveAdapter
                        };
                        let task = Task{machine: task_adapter };
                        Some(task)
                    }
                    Ok(instruction) => {
                        machine.state.set(CollectiveState::Ready);
                        // find a way to clone and assign command..
                        let adapter = Self {
                            machine: self.machine.clone(),
                            receiver: self.receiver.clone(),
                            instruction: Some(instruction),
                        };
                        let task_adapter = SharedCollectiveAdapter {
                            id: machine.id,
                            state: machine.state.clone(),
                            normalized_adapter: Box::new(adapter) as CommonCollectiveAdapter
                        };
                        let task = Task{machine: task_adapter };
                        Some(task)
                    }
                }
            }
            fn receive_cmd(&mut self, state: &MachineState, time_slice: std::time::Duration, stats: &mut ExecutorStats) {
                if state.get() == CollectiveState::Disconnected {
                    self.machine.disconnected();
                    state.set(CollectiveState::Dead);
                    return
                }
                state.set(CollectiveState::Running);
                // while we're running, might as well try to drain the queue, but keep it bounded
                let start = std::time::Instant::now();
                let mut cmd = self.instruction.take().unwrap();
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
            pub id: u128,
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
            fn get_id(&self) -> u128 { self.id }
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

        impl MachineAdapter for #adapter_ident {
            type InstructionSet = #name;

            /// Consume a raw machine, using it to create a machine that is usable by
            /// the framework.
            fn build_raw<T>(raw: T, channel_capacity: usize) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, SharedCollectiveAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition<T>(machine: &Arc<Mutex<T>>, channel_capacity: usize) -> (Sender<Self::InstructionSet>, SharedCollectiveAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_unbounded<T>(raw: T) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, SharedCollectiveAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition_unbounded<T>(machine: &Arc<Mutex<T>>) -> (Sender<Self::InstructionSet>, SharedCollectiveAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_common<T>(raw: T, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (Arc<Mutex<T>>, Sender<Self::InstructionSet>, SharedCollectiveAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // wrap it
                 let instance: Arc<Mutex<T>> = Arc::new(Mutex::new(raw));
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = Arc::clone(&instance) as Arc<dyn Machine<Self::InstructionSet>>;
                 // get the id
                 let id = COLLECTIVE_ID.fetch_add(1) as u128;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                     instruction: None,
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let shared_adapter = SharedCollectiveAdapter {
                     id: id,
                     state: MachineState::default(),
                     normalized_adapter: CommonCollectiveAdapter::from(adapter),
                 };
                 (instance, sender, shared_adapter)
            }

            fn build_addition_common<T>(machine: &Arc<Mutex<T>>, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (Sender<Self::InstructionSet>, SharedCollectiveAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = Arc::clone(machine) as Arc<dyn Machine<Self::InstructionSet>>;
                 // get the id
                 let id = COLLECTIVE_ID.fetch_add(1) as u128;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                     instruction: None,
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let shared_adapter = SharedCollectiveAdapter {
                     id: id,
                     state: MachineState::default(),
                     normalized_adapter: CommonCollectiveAdapter::from(adapter),
                 };
                 (sender, shared_adapter)
            }
        }

        // build constructs a instruction specific machine. It is converted
        // from that into a trait object that can be shared via From. Doing
        // it here, rather than in build allows for some changes in design
        // down the road.
        impl From<#adapter_ident> for CommonCollectiveAdapter {
            fn from(adapter: #adapter_ident) -> Self {
                std::boxed::Box::new(adapter) as CommonCollectiveAdapter
            }
        }
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
