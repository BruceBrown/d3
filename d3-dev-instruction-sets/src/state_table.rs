use super::*;

// cov: begin-ignore-line

#[derive(Debug, SmartDefault, MachineImpl)]
#[allow(dead_code)]
pub enum StateTable {
    #[default]
    Init,
    Start,
    Stop,
}

// cov: end-ignore-line
