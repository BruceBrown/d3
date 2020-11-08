extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse_macro_input;
use syn::DeriveInput;

/// MachineImpl is a derive macro that tranforms an enum into an instruction set that can be implemented
/// by machines.
///
/// # Example
///
/// ```text
/// #[macro_use] extern crate d3_derive;
/// use d3_core::MachineImpl::*;
///
/// // instructions can be unit-like
/// #[derive(Debug, MachineImpl)]
/// pub enum StateTable {
///     Init,
///     Start,
///     Stop,
/// }
///
/// // instructions can also be tupple, and struct
/// #[derive(Debug, MachineImpl)]
/// pub enum TrafficLight {
///     Red(TrafficLightModality),
///     Green(TrafficLightModality),
///     Yellow(TrafficLightModality),
/// }
///
/// #[derive(Debug)]
/// pub enum TrafficLightModality {
///     Solid,
///     Blinking,
/// }
///
/// Instructions can be mixed
/// #[derive(Debug, MachineImpl)]
/// pub enum Calc {
///     Clear,
///     Add(u32),
///     Sub(u32),
///     Div(u32),
///     Mul(u32),
/// }
/// ```
#[proc_macro_derive(MachineImpl)]
pub fn derive_machine_impl_fn(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let adapter_ident = format_ident!("MachineBuilder{}", name.to_string());
    let sender_adapter_ident = format_ident!("SenderAdapter{}", name.to_string());
    // let recv_wait_ident = format_ident!("RecvWait{}", name.to_string());
    let expanded = quote! {
        impl MachineImpl for #name {
            type Adapter = #adapter_ident;
            type SenderAdapter = #sender_adapter_ident;
            type InstructionSet = #name;
            fn park_sender(
                channel_id: usize,
                receiver_machine: std::sync::Weak<MachineAdapter>,
                sender: crossbeam::channel::Sender<Self::InstructionSet>,
                instruction: Self::InstructionSet) -> Result<(),Self::InstructionSet> {
                //Err(instruction)
                tls_executor_data.with(|t|{
                    let mut tls = t.borrow_mut();
                    // if its the main thread, let it block.
                    if tls.id == 0 { Err(instruction) }
                    else {
                        if let ExecutorDataField::Machine(machine) = &tls.machine {
                           let adapter = #sender_adapter_ident {
                                receiver_machine,
                                sender: sender,
                                instruction: Some(instruction),
                            };
                            let shared_adapter = MachineSenderAdapter::new(machine, Box::new(adapter));
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
        #[doc(hidden)]
        pub struct #adapter_ident {
            machine: std::sync::Arc<dyn Machine<#name>>,
            receiver: Receiver<#name>,
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
            fn receive_cmd(&self, machine: &ShareableMachine, once: bool, drop: bool, time_slice: std::time::Duration, stats: &mut ExecutorStats) {
                stats.tasks_executed += 1;
                if machine.is_disconnected() {
                    if let Err(state) = machine.compare_and_exchange_state(MachineState::Ready, MachineState::Running) {
                        log::error!("exec: disconnected: expected state = Ready, machine {} found {:#?}", machine.get_key(), state);
                        machine.set_state(MachineState::Running);
                    }
                    if once {
                        self.machine.connected(machine.get_id());
                        stats.instructs_sent += 1;
                    }
                    self.machine.disconnected();
                    stats.instructs_sent += 1;
                    if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::Dead) {
                        log::error!("exec: disconnected: expected state = Running, machine {} found {:#?}", machine.get_key(), state);
                        machine.set_state(MachineState::Dead);
                    }
                    return
                }
                if let Err(state) = machine.compare_and_exchange_state(MachineState::Ready, MachineState::Running) {
                    log::error!("exec: expected state = Ready, machine {} found {:#?}", machine.get_key(), state);
                    machine.set_state(MachineState::Running);
                }
                // while we're running, might as well try to drain the queue, but keep it bounded
                let start = std::time::Instant::now();
                if once {
                    self.machine.connected(machine.get_id());
                    stats.instructs_sent += 1;
                }
                //let mut cmd = self.take_instruction().unwrap();
                let mut count = 0;
                //log::trace!("enter chan {}, machine {} q_len {}", self.receiver.get_id(), machine.get_key(), self.receiver.receiver.len());
                loop {
                    if start.elapsed() > time_slice {
                        stats.exhausted_slice += 1;
                        break;
                    }
                    let state = machine.get_state();
                    if state != MachineState::Running {
                        log::debug!("exec: no longer running, machine {} state {:#?}", machine.get_key(), state);
                        break;
                    }
                    match self.receiver.receiver.try_recv() {
                        Ok(cmd) => {
                            self.machine.receive(cmd);
                            stats.instructs_sent += 1;
                            count += 1;
                        },
                        Err(crossbeam::channel::TryRecvError::Empty) => {
                            if drop {
                                // treat as disconnected
                                log::trace!("exec: machine {} disconnected, cleaning up", machine.get_key());
                                self.machine.disconnected();
                                stats.instructs_sent += 1;
                                if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::Dead) {
                                    log::error!("exec: (drop) expected state = Running, machine {} found {:#?}", machine.get_key(), state);
                                    machine.set_state(MachineState::Dead);
                                }
                            }
                            break;
                        },
                        Err(crossbeam::channel::TryRecvError::Disconnected) => {
                            log::trace!("exec: machine {} disconnected, cleaning up", machine.get_key());
                            self.machine.disconnected();
                            stats.instructs_sent += 1;
                            if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::Dead) {
                                log::error!("exec: (disconnected) expected state = Running, machine {} found {:#?}", machine.get_key(), state);
                                machine.set_state(MachineState::Dead);
                            }
                            break;
                        },
                    }
                }
                //log::trace!("exit chan {}, machine {} q_len {}, count {}", self.receiver.get_id(), machine.get_key(), self.receiver.receiver.len(), count);
            }
            // determine if channel is empty
            fn is_channel_empty(&self) -> bool {
                self.receiver.receiver.is_empty()
            }
            // get number of instructions in queue
            fn channel_len(&self) -> usize {
                self.receiver.receiver.len()
            }
        }
        #[doc(hidden)]
        pub struct #sender_adapter_ident {
            receiver_machine: std::sync::Weak<MachineAdapter>,
            sender: crossbeam::channel::Sender<#name>,
            instruction: Option<#name>,
        }
        impl #sender_adapter_ident {
            fn try_send(&mut self) -> Result<usize, TrySendError> {
                let instruction = self.instruction.take().unwrap();
                match self.sender.try_send(instruction) {
                    Ok(()) => {
                        if let Some(machine) = self.receiver_machine.upgrade() {
                            Ok(machine.get_key())
                        } else {
                            Err(TrySendError::Disconnected)
                        }
                    },
                    Err(crossbeam::channel::TrySendError::Disconnected(inst)) => {
                        self.instruction = Some(inst);
                        Err(TrySendError::Disconnected)
                    },
                    Err(crossbeam::channel::TrySendError::Full(inst)) => {
                        self.instruction = Some(inst);
                        Err(TrySendError::Full)
                    },
                }
            }
        }

        impl MachineDependentSenderAdapter for #sender_adapter_ident {
            fn try_send(&mut self) -> Result<usize, TrySendError> {
                match self.try_send() {
                    Ok(receiver_key) => {
                        Ok(receiver_key)
                    },
                    Err(e) => Err(e),
                }
            }
        }

        impl MachineBuilder for #adapter_ident {
            type InstructionSet = #name;
            /// Consume a raw machine, using it to create a machine that is usable by
            /// the framework.
            fn build_raw<T>(raw: T, channel_capacity: usize) -> (std::sync::Arc<parking_lot::Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition<T>(machine: &std::sync::Arc<parking_lot::Mutex<T>>, channel_capacity: usize) -> (Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel_with_capacity::<Self::InstructionSet>(channel_capacity);
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_unbounded<T>(raw: T) -> (std::sync::Arc<parking_lot::Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_common(raw, sender, receiver)
            }

            fn build_addition_unbounded<T>(machine: &std::sync::Arc<parking_lot::Mutex<T>>) -> (Sender<Self::InstructionSet>, MachineAdapter)
            where T: 'static + Machine<Self::InstructionSet>
            {
                // need to review allocation strategy for bounded
                let (sender, receiver) = channel::<Self::InstructionSet>();
                Self::build_addition_common(machine, sender, receiver)
            }

            fn build_common<T>(raw: T, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (std::sync::Arc<parking_lot::Mutex<T>>, Sender<Self::InstructionSet>, MachineAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // wrap it
                 let instance: std::sync::Arc<parking_lot::Mutex<T>> = std::sync::Arc::new(parking_lot::Mutex::new(raw));
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = std::sync::Arc::clone(&instance) as std::sync::Arc<dyn Machine<Self::InstructionSet>>;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let machine_adapter = MachineAdapter::new(Box::new(adapter));
                 (instance, sender, machine_adapter)
            }

            fn build_addition_common<T>(machine: &std::sync::Arc<parking_lot::Mutex<T>>, sender: Sender<Self::InstructionSet>, receiver: Receiver<Self::InstructionSet>) -> (Sender<Self::InstructionSet>, MachineAdapter )
                where T: 'static + Machine<Self::InstructionSet>
            {
                 // clone it, making it look like a machine, Machine for Mutex<T> facilitates this
                 let machine = std::sync::Arc::clone(machine) as std::sync::Arc<dyn Machine<Self::InstructionSet>>;
                 // wrap the machine dependent bits
                 let adapter = Self {
                     machine, receiver,
                 };
                 // wrap the independent and normalize the dependent with a trait object
                 let machine_adapter = MachineAdapter::new(Box::new(adapter));
                 (sender, machine_adapter)
            }
        }
    };
    TokenStream::from(expanded)
}
