
#[macro_use]
extern crate smart_default;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex};
use std::thread;
use std::time::Duration;

use rand::distributions::{Distribution, Uniform};
use log;

use d3_dev_instruction_sets::{TestMessage, ChaosMonkeyMutation};
use d3_core::executor;
use d3_core::machine_impl::*;

type TestMessageSender = Sender<TestMessage>;
type TestMessageReceiver = Receiver<TestMessage>;

fn wait_for_notification(receiver: &TestMessageReceiver, messages: usize, duration: Duration) {
    match receiver.recv_timeout(duration) {
        Ok(m) => {
            assert_eq!(m, TestMessage::TestData(messages));
        }
        Err(_) => {
            panic!("test failed to complete");
        }
    };
}

pub mod forwarder;
pub mod chaos_monkey;
pub mod daisy_chain;
pub mod fanout_fanin;
