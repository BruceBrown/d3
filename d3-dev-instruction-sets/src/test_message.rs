use super::*;

/// These are the event types which are exchanged for testing
#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq, MachineImpl)]
pub enum TestMessage {
    Test,
    TestData(usize),
    TestStruct(TestStruct),
    TestCallback(Sender<TestMessage>, TestStruct),
    AddSender(Sender<TestMessage>), // add a forwarding sender
    RemoveAllSenders,
    Notify(Sender<TestMessage>, usize), // notify sender, via TestData, when the count of messages have been received
    ForwardingMultiplier(usize),        // when forwarding, forward this many (use with caution)
    // Random message sending
    ChaosMonkey {
        // A counter which is either incremented or decremented
        counter: u32,
        // The max value of the counter
        counter_max: u32,
        // the type of mutation applied to the counter
        counter_mutation: ChaosMonkeyMutation,
    },
}
/// ChaosMonkey: When a forwarder receives the message it will examine the 3 values. If the first
/// value is equal to the second value, the bool is changed to false. If they aren't the same, then
/// the first value is increase if bool is true or decreased if bool is false. if the resulting value
/// is 0, a notification is sent and nothing more happens. Otherwise, a random sender is selected and
/// the modified mesage is sent to that machine.

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ChaosMonkeyMutation {
    Increment,
    Decrement,
}

/// Test structure for callback and exchange
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct TestStruct {
    pub from_id: usize,
    pub received_by: usize,
}
