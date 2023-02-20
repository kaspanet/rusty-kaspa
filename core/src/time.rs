use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the number of milliseconds since UNIX EPOCH
#[inline]
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}
