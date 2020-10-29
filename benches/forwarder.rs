//! These are the Benchmarks. We may have already done 80% of the work to minimize the time spent in
//! overhead activities, such as scheduling and executing. Now, as things change, we need to run
//! benchmarks to ensure things don't degrade.
//!
//! Once again, our old friend, the forwrder will help. We're going to run 4 benchmarks:
//! * create_destroy, where we create 4000 machines and destroy 4000 machines.
//! * daisy_chain, which send a pulse of messages to machines, stressing the executor message management.
//! * fanout-fanin, which causes send blocking, stressing the executor send blocking management.
//! * chaos-monkey, which causes random machines to have to send messages, stessing the scheduler.

#![allow(dead_code)]

use criterion::{criterion_group, criterion_main, Criterion};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use d3::core::executor;
use d3::core::machine_impl::*;
use d3_dev_instruction_sets::TestMessage;

use d3_test_drivers::chaos_monkey::ChaosMonkeyDriver;
use d3_test_drivers::daisy_chain::DaisyChainDriver;
use d3_test_drivers::fanout_fanin::FanoutFaninDriver;

pub fn bench(c: &mut Criterion) {
    setup();
    let mut group = c.benchmark_group("sched_exec_tests");
    // try to limit the length of test runs
    // group.significance_level(0.1).sample_size(10).measurement_time(Duration::from_secs(30));
    group.significance_level(0.1);

    group.bench_function("create_destroy_2000_machines", |b| b.iter(create_destroy_2000_machines));

    let mut fanout_fanin = FanoutFaninDriver::default();
    fanout_fanin.setup();
    group.bench_function("fanout_fanin_bound", |b| b.iter(|| fanout_fanin.run()));
    FanoutFaninDriver::teardown(fanout_fanin);

    let mut fanout_fanin = FanoutFaninDriver::default();
    fanout_fanin.bound_queue = false;
    fanout_fanin.setup();
    group.bench_function("fanout_fanin_unbound", |b| b.iter(|| fanout_fanin.run()));
    FanoutFaninDriver::teardown(fanout_fanin);

    let mut daisy_chain = DaisyChainDriver::default();
    daisy_chain.duration = Duration::from_secs(30);
    daisy_chain.setup();
    group.bench_function("daisy_chain_bound", |b| b.iter(|| daisy_chain.run()));
    DaisyChainDriver::teardown(daisy_chain);

    let mut daisy_chain = DaisyChainDriver::default();
    daisy_chain.bound_queue = false;
    daisy_chain.duration = Duration::from_secs(30);
    daisy_chain.setup();
    group.bench_function("daisy_chain_unbound", |b| b.iter(|| daisy_chain.run()));
    DaisyChainDriver::teardown(daisy_chain);

    let mut chaos_monkey = ChaosMonkeyDriver::default();
    chaos_monkey.setup();
    group.bench_function("chaos_monkey_bound", |b| b.iter(|| chaos_monkey.run()));
    ChaosMonkeyDriver::teardown(chaos_monkey);

    let mut chaos_monkey = ChaosMonkeyDriver::default();
    chaos_monkey.bound_queue = false;
    chaos_monkey.setup();
    group.bench_function("chaos_monkey_unbound", |b| b.iter(|| chaos_monkey.run()));
    ChaosMonkeyDriver::teardown(chaos_monkey);
    group.finish();
    teardown();
}

pub fn bench_arc(c: &mut Criterion) {
    let mut group = c.benchmark_group("arc_tests");

    // try to limit the length of test runs
    group
        .significance_level(0.1)
        .sample_size(10)
        .measurement_time(Duration::from_secs(30));

    group.bench_function("arc_test", |b| b.iter(arc_test));
    group.bench_function("unwrap_arc_test", |b| b.iter(unwrap_arc_test));
    group.bench_function("no_arc_test", |b| b.iter(no_arc_test));

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);

fn setup() {
    use simplelog::*;
    // install a simple logger
    CombinedLogger::init(vec![TermLogger::new(LevelFilter::Error, Config::default(), TerminalMode::Mixed)]).unwrap();
    executor::set_machine_count_estimate(5000);
    executor::set_default_channel_capacity(500);
    executor::start_server();
    thread::sleep(Duration::from_millis(50));
}

fn teardown() {
    executor::stop_server();
    thread::sleep(Duration::from_millis(50));
}

#[allow(non_upper_case_globals)]
static alice_generation: AtomicUsize = AtomicUsize::new(0);

// A simple Alice machine
struct Alice {
    id: usize,
}
impl Machine<TestMessage> for Alice {
    fn receive(&self, _message: TestMessage) {}
}

fn create_destroy_2000_machines() {
    // test how long it takes to create and destroy 2000 machines.
    let machine_count = 2000;
    // create the machines, save the sender
    for _ in 1 ..= machine_count {
        let alice = Alice {
            id: alice_generation.fetch_add(1, Ordering::SeqCst),
        };
        let (_, _) = executor::connect(alice);
    }
    // wait for the scheduler/executor to fully drop them
    let mut start = std::time::Instant::now();
    loop {
        thread::yield_now();
        if 0 == executor::get_machine_count() {
            break;
        }
        if start.elapsed() >= std::time::Duration::from_secs(1) {
            start = std::time::Instant::now();
            log::debug!("baseline 0, count {}", executor::get_machine_count());
        }
    }
}

fn arc_test() {
    use crossbeam::deque::Injector;
    use crossbeam::deque::Steal::{Empty, Success};
    use crossbeam::thread::scope;
    use std::sync::Arc;

    const COUNT: usize = 2_500_000;
    let q = Arc::new(Injector::<usize>::new());
    scope(|scope| {
        scope.spawn(|_| {
            for i in 0 .. COUNT {
                loop {
                    if let Success(v) = q.steal() {
                        assert_eq!(i, v);
                        break;
                    }
                }
            }
            assert_eq!(q.steal(), Empty);
        });
        for i in 0 .. COUNT {
            q.push(i);
        }
    })
    .unwrap();
}

fn unwrap_arc_test() {
    use crossbeam::deque::Injector;
    use crossbeam::deque::Steal::{Empty, Success};
    use crossbeam::thread::scope;
    use std::sync::Arc;

    const COUNT: usize = 2_500_000;
    let q = Arc::new(Injector::<usize>::new());
    let q2 = Arc::clone(&q);
    scope(|scope| {
        scope.spawn(|_| {
            let q2_ref: &Injector<usize> = &*q2;
            for i in 0 .. COUNT {
                loop {
                    if let Success(v) = q2_ref.steal() {
                        assert_eq!(i, v);
                        break;
                    }
                }
            }
            assert_eq!(q2_ref.steal(), Empty);
        });
        let q_ref: &Injector<usize> = &*q;
        for i in 0 .. COUNT {
            q_ref.push(i);
        }
    })
    .unwrap();
}
fn no_arc_test() {
    use crossbeam::deque::Injector;
    use crossbeam::deque::Steal::{Empty, Success};
    use crossbeam::thread::scope;

    const COUNT: usize = 2_500_000;
    let q = Injector::<usize>::new();
    scope(|scope| {
        scope.spawn(|_| {
            for i in 0 .. COUNT {
                loop {
                    if let Success(v) = q.steal() {
                        assert_eq!(i, v);
                        break;
                    }
                }
            }
            assert_eq!(q.steal(), Empty);
        });
        for i in 0 .. COUNT {
            q.push(i);
        }
    })
    .unwrap();
}
