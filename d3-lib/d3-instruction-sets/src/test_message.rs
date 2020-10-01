use super::*;

/// These are the event types which are exchanged for testing
#[allow(dead_code)]
#[derive(Debug, MachineImpl, Clone, Eq, PartialEq)]
pub enum TestMessage {
    Test,
    TestData(usize),
    TestStruct(TestStruct),
    TestCallback(Sender<TestMessage>, TestStruct),
    AddSender(Sender<TestMessage>),         // add a forwarding sender
    Notify(Sender<TestMessage>, usize),     // notify sender, via TestData, when the count of messages have been received
    Add(u32),
    Set(u32),
}

/// Test structure for callback and exchange
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct TestStruct {
    pub from_id: usize,
    pub received_by: usize,
}
