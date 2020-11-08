//! This crate includes commonly used machines and machine setup. They are used
//! in bench and integration tests. They can also be configured and run with the
//! td3-test-server.
#[macro_use] extern crate smart_default;
#[allow(unused_imports)]
#[macro_use]
extern crate log;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rand::distributions::{Distribution, Uniform};

use d3::core::executor;
use d3::core::machine_impl::*;
use d3_dev_instruction_sets::{ChaosMonkeyMutation, TestMessage};

type TestMessageSender = Sender<TestMessage>;
type TestMessageReceiver = Receiver<TestMessage>;

mod common;
pub use common::TestDriver;
use common::*;

pub mod chaos_monkey;
pub mod daisy_chain;
pub mod fanout_fanin;
pub mod forwarder;
