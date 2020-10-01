#![allow(dead_code)]
#[allow(unused_imports)]
use super::*;
use crate::components::ComponentSender;

///
/// CoordinatorInfo describes an active coordinator.
#[derive(Debug, Clone)]
pub struct CoordinatorInfo {
    /// Which coordinator this is
    pub coordinator: settings::Coordinator,
    /// The sender for the coordinator, it participates as a component afterall
    pub sender: ComponentSender,
}

