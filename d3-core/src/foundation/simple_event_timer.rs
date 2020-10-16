/// This is just a brain-dead simple event timer. Mostly, its used for
/// determining when to report statistics. It defaults to once every
/// 5 minutes. It is designed to account for drift and long sleeps
/// where one or more expirations may be missed.
use std::time::{Duration, Instant};

#[derive(Debug, SmartDefault, Copy, Clone)]
pub struct SimpleEventTimer {
    #[default(Duration::from_secs(300))]
    duration: Duration,

    #[default(Instant::now())]
    last_event: Instant,
}
impl SimpleEventTimer {
    // new allows a different duration, but not less than a second
    #[allow(dead_code)]
    pub fn new(duration: Duration) -> Self {
        if duration < Duration::from_secs(1) {
            panic!("SimpleEventTimer must have a duration of a second or more.");
        }
        Self {
            duration,
            ..Self::default()
        }
    }
    pub fn check(&mut self) -> bool {
        if self.last_event.elapsed() >= self.duration {
            // may need to advance several iterations if we've been asleep
            loop {
                self.last_event += self.duration;
                if self.last_event.elapsed() < self.duration {
                    break;
                }
            }
            true
        } else {
            false
        }
    }
}
