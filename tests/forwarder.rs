//#![feature(test)]
// extern crate test;

#[macro_use] extern crate log;

#[cfg(test)]
mod tests {
    use std::panic;
    use std::thread;
    use std::time::Duration;

    use d3::core::executor::{self, *};
    use d3::core::machine_impl::*;
    use d3_dev_instruction_sets::{TestMessage, TestStruct};

    use d3_test_drivers::chaos_monkey::ChaosMonkeyDriver;
    use d3_test_drivers::daisy_chain::DaisyChainDriver;
    use d3_test_drivers::fanout_fanin::FanoutFaninDriver;
    use d3_test_drivers::forwarder::Forwarder;
    use d3_test_drivers::TestDriver;

    // common function for wrapping a test with setup/teardown logic
    pub fn run_test<T>(test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        // tweaks for more responsive testing
        executor::set_machine_count_estimate(5000);
        executor::set_default_channel_capacity(500);
        // if let Err(err) = CombinedLogger::init(vec![
        // TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed),
        // WriteLogger::new(LevelFilter::Trace, Config::default(), File::create("rust_test.log").unwrap()),
        // ]) {
        // println!("logging init error: {}", err);
        // }
        setup();
        let result = panic::catch_unwind(|| test());
        teardown();
        assert!(result.is_ok())
    }

    fn setup() {
        info!("setup starting server");
        executor::start_server()
    }

    fn teardown() {
        info!("teardown stopping server");
        executor::stop_server();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    #[test]
    fn create_destroy() {
        // A simple Alice machine
        struct Alice {}
        impl Machine<TestMessage> for Alice {
            fn receive(&self, _message: TestMessage) {}
        }

        run_test(|| {
            let machine_count = 10;
            let mut machines: Vec<Sender<TestMessage>> = Vec::with_capacity(machine_count);
            let baseline_machine_count = get_machine_count();
            // build a bunch of alice's
            for _ in 1 ..= machine_count {
                let alice = Alice {};
                let (_, sender) = executor::connect(alice);
                machines.push(sender);
            }
            // wait for them to get connected to the scheduler
            loop {
                std::thread::yield_now();
                if get_machine_count() == baseline_machine_count + machine_count {
                    break;
                }
            }
            machines.clear();
            // wait for the scheduler/executor to disconnect them all
            loop {
                std::thread::yield_now();
                if get_machine_count() == baseline_machine_count {
                    break;
                }
            }
        });
    }

    #[test]
    fn can_receive() {
        run_test(|| {
            let f = Forwarder::default();
            let (t, s) = executor::connect(f);
            s.send(TestMessage::Test).unwrap();
            // check if we've received it, but only for a second
            let start = std::time::Instant::now();
            loop {
                thread::sleep(Duration::from_millis(10));
                let count = t.get_and_clear_received_count();
                if count == 1 {
                    break;
                }
                if start.elapsed() > std::time::Duration::from_secs(1) {
                    assert_eq!(true, false);
                }
            }
            // we got the count of 1, now check that its cleared
            assert_eq!(t.get_and_clear_received_count(), 0);
        });
    }

    #[test]
    fn can_callback() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = executor::connect(f);
            let (sender, r) = channel();
            let mut data = TestStruct::default();
            data.from_id = 10000;
            s.send(TestMessage::TestCallback(sender, data)).unwrap();
            let m = r.recv_timeout(Duration::from_secs(1)).expect("data didn't arrive");
            match m {
                TestMessage::TestStruct(results) => {
                    assert_eq!(results.from_id, data.from_id);
                    assert_eq!(results.received_by, 1);
                },
                _ => assert_eq!(true, false),
            }
        });
    }

    #[test]
    fn can_forward() {
        run_test(|| {
            log::info!("can_forward running");
            let f = Forwarder::new(1);
            let (_t, s) = executor::connect(f);
            let (sender, r) = channel();
            s.send(TestMessage::AddSender(sender)).unwrap();
            s.send(TestMessage::Test).unwrap();
            let m = r.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
        });
    }

    #[test]
    fn can_notify() {
        run_test(|| {
            let f = Forwarder::new(1);
            let (_t, s) = executor::connect(f);
            let (sender, r) = channel();
            s.send(TestMessage::Notify(sender, 2)).unwrap();
            s.send(TestMessage::Test).unwrap();
            match r.recv_timeout(Duration::from_millis(100)) {
                Ok(_m) => panic!("notification arrived early"),
                Err(_e) => (),
            }
            s.send(TestMessage::Test).unwrap();
            match r.recv_timeout(Duration::from_secs(1)) {
                Ok(m) => assert_eq!(m, TestMessage::TestData(2)),
                Err(e) => panic!("unexpected error '{}' while waiting for data", e),
            }
        });
    }

    #[test]
    fn can_fanout() {
        run_test(|| {
            log::info!("can_fanout running");
            let f = Forwarder::new(1);
            let (_t, s) = executor::connect(f);
            let (sender1, r1) = channel();
            let (sender2, r2) = channel();
            s.send(TestMessage::AddSender(sender1)).unwrap();
            s.send(TestMessage::AddSender(sender2)).unwrap();
            s.send(TestMessage::Test).unwrap();
            let m = r1.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
            let m = r2.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(m, TestMessage::Test);
        });
    }

    #[test]
    fn daisy_chain() {
        // Previous tests was a proof of functionality. Now, we're going to stress things.
        run_test(|| {
            log::info!("daisy_chain running");
            assert_eq!(executor::get_machine_count(), 0);
            let mut daisy_chain = DaisyChainDriver::default();
            daisy_chain.machine_count = 1000;
            daisy_chain.message_count = 10;
            daisy_chain.duration = Duration::from_secs(60);
            let iterations = 2;
            daisy_chain.setup();
            log::info!("starting run");
            let start = std::time::Instant::now();
            for _ in 0 .. iterations {
                daisy_chain.run();
            }
            log::info!("completed, duration {:#?}", start.elapsed());
            DaisyChainDriver::teardown(daisy_chain);
        });
    }

    #[test]
    fn multiplier() {
        // Previous tests was a proff of functionality. Now, we're going to stress things.
        run_test(|| {
            log::info!("multiplier running");
            assert_eq!(executor::get_machine_count(), 0);
            let mut daisy_chain = DaisyChainDriver::default();
            daisy_chain.machine_count = 8;
            daisy_chain.message_count = 1;
            daisy_chain.forwarding_multiplier = 4;
            daisy_chain.duration = Duration::from_secs(15);
            let iterations = 2;
            daisy_chain.setup();
            for _ in 0 .. iterations {
                daisy_chain.run();
            }
            DaisyChainDriver::teardown(daisy_chain);
        });
    }

    #[test]
    fn fanout_fanin() {
        run_test(|| {
            log::info!("fanout_fanin running");
            assert_eq!(executor::get_machine_count(), 0);
            let mut fanout_fanin = FanoutFaninDriver::default();
            fanout_fanin.machine_count = 500;
            fanout_fanin.message_count = 15;
            fanout_fanin.duration = std::time::Duration::from_secs(30);
            let iterations = 2;
            fanout_fanin.setup();
            for i in 0 .. iterations {
                fanout_fanin.run();
                log::info!("iteration {} of {} completed", i + 1, iterations);
            }
            FanoutFaninDriver::teardown(fanout_fanin);
        });
    }

    #[test]
    fn chaos_monkey() {
        // use simplelog::*;
        // use std::fs::File;
        // if let Err(err) = CombinedLogger::init(vec![
        // TermLogger::new(LevelFilter::Warn, Config::default(), TerminalMode::Mixed),
        // WriteLogger::new(LevelFilter::Trace, Config::default(), File::create("rust_test.log").unwrap()),
        // ]) {
        // println!("logging init error: {}", err);
        // }
        run_test(|| {
            let machine_count = executor::get_machine_count();
            if machine_count != 0 {
                log::error!("chaos_monkey expecting machine_count of 0, found {}", machine_count);
                executor::stats::request_machine_info();
                thread::sleep(Duration::from_secs(2));
                assert_eq!(machine_count, 0);
            }
            let mut chaos_monkey = ChaosMonkeyDriver::default();
            chaos_monkey.machine_count = 1000;
            chaos_monkey.message_count = 50;
            chaos_monkey.inflection_value = 19;
            chaos_monkey.bound_queue = false;
            chaos_monkey.duration = std::time::Duration::from_secs(30);
            let iterations = 2;
            chaos_monkey.setup();
            log::info!("starting run");
            let start = std::time::Instant::now();
            for _ in 0 .. iterations {
                chaos_monkey.run();
            }
            log::info!("completed, duration {:#?}", start.elapsed());
            ChaosMonkeyDriver::teardown(chaos_monkey);
        });
    }
}
