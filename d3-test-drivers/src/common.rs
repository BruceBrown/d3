use super::*;

// Common functions and traits which are used by all tests

pub trait TestDriver {
    fn setup(&mut self);
    fn teardown(driver: Self);
    fn run(&self);
}

// wait for a notification to arrive, indicating that a run iteration completed
pub fn wait_for_notification(receiver: &TestMessageReceiver, messages: usize, duration: Duration) -> Result<(), ()> {
    let start = Instant::now();
    if let Ok(m) = receiver.recv_timeout(duration) {
        assert_eq!(m, TestMessage::TestData(messages));
        Ok(())
    } else {
        log::error!("wait_for_notification failed, started at {:#?}", start);
        log::error!("run queue len {}", executor::get_run_queue_len());
        d3::core::executor::stats::request_stats_now();
        d3::core::executor::stats::request_machine_info();
        thread::sleep(Duration::from_millis(100));
        Err(())
    }
}

// wait for the machine count to be increased to the provided count
pub fn wait_for_machine_setup(machine_count: usize) -> Result<(), ()> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_nanos(50));
        if executor::get_machine_count() >= machine_count {
            return Ok(());
        }
    }
    if log_enabled!(log::Level::Error) {
        log::error!(
            "wait_for_machine_setup failed: count={}, expecting={}",
            executor::get_machine_count(),
            machine_count
        );
    } else {
        println!(
            "wait_for_machine_setup failed: count={}, expecting={}",
            executor::get_machine_count(),
            machine_count
        );
    }
    Err(())
}

// wait for the machine count to be reduced to the provided count
pub fn wait_for_machine_teardown(machine_count: usize) -> Result<(), ()> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_nanos(50));
        if executor::get_machine_count() <= machine_count {
            return Ok(());
        }
    }
    if log_enabled!(log::Level::Error) {
        log::error!(
            "wait_for_machine_teardown failed: count={}, expecting={}",
            executor::get_machine_count(),
            machine_count
        );
    } else {
        println!(
            "wait_for_machine_teardown failed: count={}, expecting={}",
            executor::get_machine_count(),
            machine_count
        );
    }
    Err(())
}
