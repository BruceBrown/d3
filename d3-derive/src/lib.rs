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
        // cov: begin-ignore-line
        impl MachineImpl for #name {
            type Adapter = #adapter_ident;
            type SenderAdapter = #sender_adapter_ident;
            type InstructionSet = #name;
            fn park_sender(
                channel_id: usize,
                receiver_machine: std::sync::Arc<MachineAdapter>,
                sender: crossbeam::channel::Sender<Self::InstructionSet>,
                instruction: Self::InstructionSet) -> Result<(),Self::InstructionSet> {
                //Err(instruction)
                tls_executor_data.with(|t|{
                    let mut tls = t.borrow_mut();
                    // if its the main thread, let it block.
                    if tls.id == 0 { Err(instruction) }
                    else {
                        let machine = &tls.machine;
                        let adapter = #sender_adapter_ident {
                            receiver_machine,
                            sender: sender,
                            instruction: Some(instruction),
                        };
                        let shared_adapter = MachineSenderAdapter::new(machine, Box::new(adapter));
                        tls.sender_blocked(channel_id, shared_adapter);
                        Ok(())
                    }
                })
            }
        }
        // cov: end-ignore-line

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
        // cov: begin-ignore-line
        impl #adapter_ident {
            #[inline]
            fn ready_to_running(machine: &ShareableMachine) {
                if let Err(state) = machine.compare_and_exchange_state(MachineState::Ready, MachineState::Running) {
                    log::error!("exec: disconnected: expected state = Ready, machine {} found {:#?}", machine.get_key(), state);
                    machine.set_state(MachineState::Running);
                }
            }
            #[inline]
            fn handle_once(&self, machine: &ShareableMachine, once: bool, stats: &mut ExecutorStats) {
                if once {
                    self.machine.connected(machine.get_id());
                    stats.instructs_sent += 1;
                }
            }
            #[inline]
            fn handle_disconnect(&self, machine: &ShareableMachine, stats: &mut ExecutorStats) {
                self.machine.disconnected();
                stats.instructs_sent += 1;
                if let Err(state) = machine.compare_and_exchange_state(MachineState::Running, MachineState::Dead) {
                    log::error!("exec: disconnected: expected state = Running, machine {} found {:#?}", machine.get_key(), state);
                    machine.set_state(MachineState::Dead);
                }
            }
            fn recv_cmd(&self, machine: &ShareableMachine, once: bool, time_slice: std::time::Duration, stats: &mut ExecutorStats) {
                stats.tasks_executed += 1;

                Self::ready_to_running(machine);
                // while we're running, might as well try to drain the queue, but keep it bounded
                let start = std::time::Instant::now();
                self.handle_once(machine, once, stats);
                let mut count = 0;
                //log::trace!("enter chan {}, machine {} q_len {}", self.receiver.get_id(), machine.get_key(), self.receiver.receiver.len());
                loop {
                    // occasionally check for exhausting slice
                    if count % 8 == 7 && start.elapsed() > time_slice {
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
                            if machine.is_disconnected() {
                                // treat as disconnected
                                self.handle_disconnect(machine, stats);
                            }
                            break;
                        },
                        Err(crossbeam::channel::TryRecvError::Disconnected) => {
                            log::trace!("exec: machine {} disconnected, cleaning up", machine.get_key());
                            self.handle_disconnect(machine, stats);
                            break;
                        },
                    }
                }
                //log::trace!("exit chan {}, machine {} q_len {}, count {}", self.receiver.get_id(), machine.get_key(), self.receiver.receiver.len(), count);
            }
        }
        // cov: end-ignore-line

        // cov: begin-ignore-line
        impl std::fmt::Debug for #adapter_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "#adapter_ident {{ .. }}")
            }
        }
        // cov: end-ignore-line

        // cov: begin-ignore-line
        impl MachineBuilder for #adapter_ident {
            type InstructionSet = #name;
            /// Consume a raw machine and receiver, using them to construct 3 closures:
            /// is_channel_empty -- returns true if the receiver channel is empty
            /// get_channel_len -- returns the count of commands in the receiver channel
            /// recv_cmd -- receives commands and pushes them into the machine
            ///
            /// All of the closuers are injected into the adapter, as an alternative to
            /// providing a trait object.
            ///
            fn build_adapter(machine: std::sync::Arc<dyn Machine<Self::InstructionSet>>, receiver: Receiver<Self::InstructionSet>) -> MachineAdapter
            {
                let r = receiver.receiver.clone();
                let is_channel_empty = Box::new(move || -> bool { r.is_empty() });

                let r = receiver.receiver.clone();
                let get_channel_len = Box::new(move || -> usize { r.len() });

                let adapter = Self { machine, receiver, };
                let recv_cmd = Box::new(move |machine: &ShareableMachine, once: bool, time_slice: std::time::Duration, stats: &mut ExecutorStats| {
                    adapter.recv_cmd(machine, once, time_slice, stats);
                });
                MachineAdapter::new(is_channel_empty, get_channel_len, recv_cmd)
            }
        }
        // cov: end-ignore-line

        // This is the generic adapter implementation for the adapter, much of this is
        // already generic, so maybe there's an alternative where the dyn stuff can
        // be used less often.
        //

        // cov: begin-ignore-line
        impl MachineDependentAdapter for #adapter_ident {
            fn receive_cmd(&self, machine: &ShareableMachine, once: bool, time_slice: std::time::Duration, stats: &mut ExecutorStats) {
                self.recv_cmd(machine, once, time_slice, stats);
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
        // cov: end-ignore-line

        #[doc(hidden)]
        pub struct #sender_adapter_ident {
            receiver_machine: std::sync::Arc<MachineAdapter>,
            sender: crossbeam::channel::Sender<#name>,
            instruction: Option<#name>,
        }

        // cov: begin-ignore-line
        impl #sender_adapter_ident {
            fn try_send(&mut self) -> Result<usize, TrySendError> {
                let instruction = self.instruction.take().unwrap();
                match self.sender.try_send(instruction) {
                    Ok(()) =>
                           Ok(self.receiver_machine.get_key())
                    ,
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
        // cov: end-ignore-line

        // cov: begin-ignore-line
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
        // cov: end-ignore-line

    };
    TokenStream::from(expanded)
}
