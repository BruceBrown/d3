use super::*;

/// The TestMessage instruction set is an example of an instruction set that is both
/// condensed and yet complete enough to be implemented by the Forwarder for integration
/// and bench tests.
/// # examples
///
/// ```
/// # #[allow(unused_imports)] use std::sync::{Arc, Mutex};
/// # use d3_core::machine_impl::*;
/// # #[allow(unused_imports)] use d3_derive::*;
/// #[allow(dead_code)]
/// #[derive(Debug, Clone, Eq, PartialEq, MachineImpl)]
/// pub enum TestMessage {
///     // Test is a unit-like instruction with no parameters
///     Test,
///     // TestData is a tuple instruction with a single parameter
///     TestData(usize),
///     // TestStruct is an example of passing a structure
///     TestStruct(TestStruct),
///     // TestCallback illustrates passing a sender and a structure to be sent back to the sender
///     TestCallback(Sender<TestMessage>, TestStruct),
///     // AddSender can be implemented to push a sender onto a list of senders
///     AddSender(Sender<TestMessage>),
///     // RemoveAllSeners can be implemented to clear list of senders
///     RemoveAllSenders,
///     // Notify, is setup for a notification via TestData, where usize is a message count
///     Notify(Sender<TestMessage>, usize),
///     // ForwardingMultiplier provides a parameter to the forwarder
///     ForwardingMultiplier(usize),
///     // ChaosMonkey is a struct variant containing 3 fields
///     ChaosMonkey {
///         // A counter which is either incremented or decremented
///         counter: u32,
///         // The maximum value of the counter
///         max: u32,
///         // the type of mutation applied to the counter
///         mutation: ChaosMonkeyMutation,
///     },
/// }
///
/// // TestMessageSender is shorthand for a sender of a TestMessage instruction.
/// pub type TestMessageSender = Sender<TestMessage>;
///
/// // Test structure for callback and exchange
/// #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
/// pub struct TestStruct {
///     pub from_id: usize,
///     pub received_by: usize,
/// }
/// // The type of mutation to apply to the chaos monkey counter
/// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// pub enum ChaosMonkeyMutation {
///     Increment,
///     Decrement,
/// }
/// ```
/// When warranted, an implementation for one or more instructions
/// can be provided. This illustrates advancing TestMessage::ChaosMonkey.
/// ```
/// # #[allow(unused_imports)] use std::sync::{Arc, Mutex};
/// # use d3_core::machine_impl::*;
/// # #[allow(unused_imports)] use d3_derive::*;
/// # #[allow(dead_code)]
/// # #[derive(Debug, Clone, Eq, PartialEq, MachineImpl)]
/// # pub enum TestMessage {
/// #    ChaosMonkey {
/// #        // A counter which is either incremented or decremented
/// #        counter: u32,
/// #        // The maximum value of the counter
/// #        max: u32,
/// #        // the type of mutation applied to the counter
/// #        mutation: ChaosMonkeyMutation,
/// #    },
/// # }
/// # #[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// # pub enum ChaosMonkeyMutation {
/// #    Increment,
/// #    Decrement,
/// # }
///
/// impl TestMessage {
///     // The advance method returns self or for those instructions that
///     // can be advanced, it returns the result of advancing.
///     pub fn advance(self) -> Self {
///         match self {
///             Self::ChaosMonkey {
///                 counter,
///                 max,
///                 mutation
///             } => Self::advance_chaos_monkey(counter, max, mutation),
///             _ => self,
///         }
///     }
///     // Advance the chaos monkey variant by increment the counter until it reaches
///     // its maximum value, then decrement it. Once the counter reaches 0, no further
///     // advancement is performed.
///     fn advance_chaos_monkey(counter: u32, max: u32, mutation: ChaosMonkeyMutation) -> Self {
///         match counter {
///             0 => match mutation {
///                 ChaosMonkeyMutation::Increment => TestMessage::ChaosMonkey {
///                     counter: counter + 1,
///                     max,
///                     mutation,
///                 },
///                 ChaosMonkeyMutation::Decrement => TestMessage::ChaosMonkey {
///                     counter,
///                     max,
///                     mutation,
///                 },
///             },
///             c if c >= max => match mutation {
///                 ChaosMonkeyMutation::Increment => TestMessage::ChaosMonkey {
///                     counter,
///                     max,
///                     mutation: ChaosMonkeyMutation::Decrement,
///                 },
///                 ChaosMonkeyMutation::Decrement => TestMessage::ChaosMonkey {
///                     counter: counter - 1,
///                     max,
///                     mutation,
///                 },
///             },
///             _ => match mutation {
///                 ChaosMonkeyMutation::Increment => TestMessage::ChaosMonkey {
///                     counter: counter + 1,
///                     max,
///                     mutation,
///                 },
///                 ChaosMonkeyMutation::Decrement => TestMessage::ChaosMonkey {
///                     counter: counter - 1,
///                     max,
///                     mutation,
///                 },
///             },
///         }
///     }
/// }
///
/// let v = TestMessage::ChaosMonkey{
///     counter: 0,
///     max: 1,
///     mutation: ChaosMonkeyMutation::Increment,
///     };
/// // ensure that we start correctly
/// if let TestMessage::ChaosMonkey { counter, max, mutation } = v {
///     assert_eq!(counter, 0);
///     assert_eq!(max, 1);
///     assert_eq!(mutation, ChaosMonkeyMutation::Increment);
/// } else { assert_eq!(true, false) }
///
/// // advance until advancing has no effect
/// let v = v.advance();
/// if let TestMessage::ChaosMonkey{counter, max, mutation} = v {
///     assert_eq!(counter, 1);
///     assert_eq!(max, 1);
///     assert_eq!(mutation, ChaosMonkeyMutation::Increment);
/// } else { assert_eq!(true, false) }
///
/// let v = v.advance();
/// if let TestMessage::ChaosMonkey{counter, max, mutation} = v {
///     assert_eq!(counter, 1);
///     assert_eq!(max, 1);
///     assert_eq!(mutation, ChaosMonkeyMutation::Decrement);
/// } else { assert_eq!(true, false) }
///
/// let v = v.advance();
/// if let TestMessage::ChaosMonkey{counter, max, mutation} = v {
///     assert_eq!(counter, 0);
///     assert_eq!(max, 1);
///     assert_eq!(mutation, ChaosMonkeyMutation::Decrement);
/// } else { assert_eq!(true, false) }
///
/// // no change
/// let v = v.advance();
/// if let TestMessage::ChaosMonkey{counter, max, mutation} = v {
///     assert_eq!(counter, 0);
///     assert_eq!(max, 1);
///     assert_eq!(mutation, ChaosMonkeyMutation::Decrement);
/// } else { assert_eq!(true, false) }
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq, MachineImpl)]
pub enum TestMessage {
    // Test is a unit-like instruction with no parameters
    Test,
    // TestData has a single parameter, as a tuple
    TestData(usize),
    // TestStruct is an example of passing a structure
    TestStruct(TestStruct),
    // TestCallback illustrates passing a sender and a structure to be sent back to the sender
    TestCallback(TestMessageSender, TestStruct),
    // AddSender can be implemented to push a sender onto a list of senders
    AddSender(TestMessageSender),
    // RemoveAllSeners can be implemented to clear list of senders
    RemoveAllSenders,
    // Notify, is setup for a notification via TestData, where usize is a message count
    Notify(TestMessageSender, usize),
    // ForwardingMultiplier provides a parameter to the forwarder
    ForwardingMultiplier(usize),
    // Random message sending, illustrates that a variant struct can be used as well as a tuple
    ChaosMonkey {
        // A counter which is either incremented or decremented
        counter: u32,
        // The maximum value of the counter
        max: u32,
        // the type of mutation applied to the counter
        mutation: ChaosMonkeyMutation,
    },
}
#[allow(dead_code)]
impl TestMessage {
    // The advance method returns self or for those instructions that can be advanced, it returns the result of advancing.
    // clippy misunderstands and belives this can be a const fn
    #[allow(clippy::missing_const_for_fn)]
    pub fn advance(self) -> Self {
        match self {
            Self::ChaosMonkey { counter, max, mutation } => Self::advance_chaos_monkey(counter, max, mutation),
            _ => self,
        }
    }
    // return true if advancing will mutate, false if advance has no effect
    pub fn can_advance(&self) -> bool {
        match self {
            Self::ChaosMonkey { counter, mutation, .. } => *counter != 0 || mutation != &ChaosMonkeyMutation::Decrement,
            _ => false,
        }
    }
    // Advance the chaos monkey variant by increment the counter until it reaches its maximum value, then decrement it.
    // Once the counter reaches 0, no further advancement is performed.
    const fn advance_chaos_monkey(counter: u32, max: u32, mutation: ChaosMonkeyMutation) -> Self {
        match counter {
            0 => match mutation {
                ChaosMonkeyMutation::Increment => Self::ChaosMonkey {
                    counter: counter + 1,
                    max,
                    mutation,
                },
                ChaosMonkeyMutation::Decrement => Self::ChaosMonkey { counter, max, mutation },
            },
            c if c >= max => match mutation {
                ChaosMonkeyMutation::Increment => Self::ChaosMonkey {
                    counter,
                    max,
                    mutation: ChaosMonkeyMutation::Decrement,
                },
                ChaosMonkeyMutation::Decrement => Self::ChaosMonkey {
                    counter: counter - 1,
                    max,
                    mutation,
                },
            },
            _ => match mutation {
                ChaosMonkeyMutation::Increment => Self::ChaosMonkey {
                    counter: counter + 1,
                    max,
                    mutation,
                },
                ChaosMonkeyMutation::Decrement => Self::ChaosMonkey {
                    counter: counter - 1,
                    max,
                    mutation,
                },
            },
        }
    }
}
// TestMessageSender is shorthand for a sender of a TestMessage instruction.
pub type TestMessageSender = Sender<TestMessage>;

// ChaosMonkey: When a forwarder receives the message it will examine the 3 values. If the first
// value is equal to the second value, the bool is changed to false. If they aren't the same, then
// the first value is increase if bool is true or decreased if bool is false. if the resulting value
// is 0, a notification is sent and nothing more happens. Otherwise, a random sender is selected and
// the modified mesage is sent to that machine.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ChaosMonkeyMutation {
    Increment,
    Decrement,
}

/// Test structure for callback and other exchanges via an external structure
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct TestStruct {
    pub from_id: usize,
    pub received_by: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_chaos_monkey_variant() {
        let v = TestMessage::ChaosMonkey {
            counter: 0,
            max: 1,
            mutation: ChaosMonkeyMutation::Increment,
        };
        assert_eq!(true, v.can_advance());
        if let TestMessage::ChaosMonkey { counter, max, mutation } = v {
            assert_eq!(counter, 0);
            assert_eq!(max, 1);
            assert_eq!(mutation, ChaosMonkeyMutation::Increment);
        } else {
            assert_eq!(true, false)
        }
    }
    #[test]
    fn test_advance() {
        let v = TestMessage::ChaosMonkey {
            counter: 0,
            max: 1,
            mutation: ChaosMonkeyMutation::Increment,
        };
        let v = v.advance();
        if let TestMessage::ChaosMonkey { counter, max, mutation } = v {
            assert_eq!(counter, 1);
            assert_eq!(max, 1);
            assert_eq!(mutation, ChaosMonkeyMutation::Increment);
        } else {
            assert_eq!(true, false)
        }
        assert_eq!(true, v.can_advance());
    }
    #[test]
    fn test_advance_ends() {
        let v = TestMessage::ChaosMonkey {
            counter: 0,
            max: 1,
            mutation: ChaosMonkeyMutation::Increment,
        };
        let v = v.advance();
        let v = v.advance();
        assert_eq!(true, v.can_advance());
        let v = v.advance();
        if let TestMessage::ChaosMonkey { counter, max, mutation } = v {
            assert_eq!(counter, 0);
            assert_eq!(max, 1);
            assert_eq!(mutation, ChaosMonkeyMutation::Decrement);
        } else {
            assert_eq!(true, false)
        }
        assert_eq!(false, v.can_advance());
        let v = v.advance();
        if let TestMessage::ChaosMonkey { counter, max, mutation } = v {
            assert_eq!(counter, 0);
            assert_eq!(max, 1);
            assert_eq!(mutation, ChaosMonkeyMutation::Decrement);
        } else {
            assert_eq!(true, false)
        }
    }
}
