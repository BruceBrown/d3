//! This crate includes commonly used machines and machine setup. They are used
//! in bench and integration tests. They can also be configured and run with the
//! td3-test-server.
#[macro_use] extern crate smart_default;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use rand::distributions::{Distribution, Uniform};

use d3::core::executor;
use d3::core::machine_impl::*;
use d3_dev_instruction_sets::{ChaosMonkeyMutation, TestMessage};

type TestMessageSender = Sender<TestMessage>;
type TestMessageReceiver = Receiver<TestMessage>;

fn wait_for_notification(receiver: &TestMessageReceiver, messages: usize, duration: Duration) {
    let start = Instant::now();
    loop {
        if start.elapsed() >= duration {
            log::warn!("run queue len {}", executor::get_run_queue_len());
            d3::core::executor::stats::request_stats_now();
            d3::core::executor::stats::request_machine_info();
            thread::sleep(Duration::from_millis(100));
            panic!("test failed to complete");
        }
        if let Ok(m) = receiver.recv_timeout(Duration::from_secs(20)) {
            assert_eq!(m, TestMessage::TestData(messages));
            break;
        }
    }
}

pub mod chaos_monkey;
pub mod daisy_chain;
pub mod fanout_fanin;
pub mod forwarder;
