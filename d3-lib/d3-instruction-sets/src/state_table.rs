use super::*;

#[derive(Copy, Clone, Debug, Eq, MachineImpl, PartialEq)]
#[allow(dead_code)]
pub enum StateTable {
    Start,
    Stop,
}
