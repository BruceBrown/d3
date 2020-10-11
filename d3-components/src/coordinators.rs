#![allow(dead_code)]
#[allow(unused_imports)]
use super::*;
use crate::components::ComponentSender;

/// CoordinatorInfo describes an active coordinator. It provides the coordinator type
/// and the sender for the coordinator. A coordinator, is a specialization of a
/// component, and shares the same instruction set as all other components and coordinators.
#[derive(Debug, Clone)]
pub struct CoordinatorInfo {
    coordinator: settings::Coordinator,
    sender: ComponentSender,
}
impl CoordinatorInfo {
    pub fn new(coordinator: settings::Coordinator, sender: ComponentSender) -> Self {
        Self {
            coordinator,
            sender,
        }
    }
    pub fn coordinator(&self) -> settings::Coordinator {
        self.coordinator
    }
    pub fn sender(&self) -> &ComponentSender {
        &self.sender
    }
}
