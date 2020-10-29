use super::*;

const MAX_YIELD: u8 = 16;

// This is a yeild lock, where we're going to back off the CPU(s) using
// yeild, and likely parking if the yield completes. The yield loop
// will perform an additional yeild per snooze, until reaching the max.
// Therefore a completed yeild will have yeilded 1+2+3..+15+16 = 136.
#[derive(Debug, Default)]
pub struct LinearBackoff {
    count: AtomicCell<u8>,
    disturbed: AtomicCell<usize>,
    completed: AtomicCell<usize>,
}
impl LinearBackoff {
    pub fn new() -> Self { Self::default() }
    // yeild, keeping track of sequence
    pub fn snooze(&self) {
        if !self.is_completed() {
            self.count.fetch_add(1);
        }
        let count = self.count.load();
        for _ in 0 .. count {
            thread::yield_now();
        }
    }
    // reset the sequence, return true if any napping was done, but not completed
    pub fn reset(&self) -> bool {
        let napped = self.did_sleep() && !self.is_completed();
        if napped {
            self.disturbed.fetch_add(1);
        }
        self.count.store(0);
        napped
    }
    // determine is sequence has been completed
    pub fn is_completed(&self) -> bool { self.count.load() == MAX_YIELD }
    // determine if any sleeping occurred since reset
    pub fn did_sleep(&self) -> bool { self.count.load() > 0 }
}
