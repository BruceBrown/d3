use super::*;

/// These are the event types which are exchanged for testing
#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq, MachineImpl)]
pub enum TestMessage {
    Test,
    TestData(usize),
    TestStruct(TestStruct),
    TestCallback(Sender<TestMessage>, TestStruct),
    AddSender(Sender<TestMessage>),         // add a forwarding sender
    Notify(Sender<TestMessage>, usize),     // notify sender, via TestData, when the count of messages have been received
    ForwardingMultiplier(usize),            // when forwarding, forward this many (use with caution)
}

/// Test structure for callback and exchange
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TestStruct {
    pub from_id: usize,
    pub received_by: usize,
}
