use super::*;

#[derive(Debug, MachineImpl)]
#[allow(dead_code)]
pub enum StateTable {
    Start,
    Stop,
}
