use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Returns the number of milliseconds since UNIX EPOCH
#[inline]
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// Stopwatch which reports on drop if the timed operation passed the threshold `TR` in milliseconds
pub struct Stopwatch<const TR: u64 = 1000> {
    name: &'static str,
    start: Instant,
}

impl Stopwatch {
    pub fn new(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }
}

impl<const TR: u64> Stopwatch<TR> {
    pub fn with_threshold(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl<const TR: u64> Drop for Stopwatch<TR> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        if elapsed > Duration::from_millis(TR) {
            kaspa_core::trace!("[{}] Abnormal time: {:#?}", self.name, elapsed);
        }
    }
}
