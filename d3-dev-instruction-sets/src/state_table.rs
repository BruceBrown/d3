use super::*;

#[derive(Debug, SmartDefault, MachineImpl)]
#[allow(dead_code)]
pub enum StateTable {
    #[default]
    Init,
    Start,
    Stop,
}
