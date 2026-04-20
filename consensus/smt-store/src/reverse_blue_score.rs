use zerocopy::byteorder::big_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// Reversed blue score for latest-first iteration in RocksDB.
///
/// Stored as `u64::MAX - blue_score` in big-endian so that forward
/// lexicographic iteration yields versions from highest actual
/// blue score to lowest. This eliminates the need for `seek_for_prev`:
///
/// To find the newest version at or before `target`, simply
/// `seek(prefix | entity | ReverseBlueScore::new(target) | 0x00..00)`
/// — the first match has `actual_blue_score <= target`.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct ReverseBlueScore(U64);

impl ReverseBlueScore {
    pub const ZERO: Self = Self(U64::ZERO);

    #[inline]
    pub fn new(blue_score: u64) -> Self {
        Self(U64::new(u64::MAX - blue_score))
    }

    #[inline]
    pub fn blue_score(&self) -> u64 {
        u64::MAX - self.0.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerocopy::IntoBytes;

    #[test]
    fn round_trip() {
        for s in [0, 1, 42, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            assert_eq!(ReverseBlueScore::new(s).blue_score(), s);
        }
    }

    #[test]
    fn ordering_latest_first() {
        let high = ReverseBlueScore::new(200);
        let low = ReverseBlueScore::new(100);
        // Higher actual blue score should sort BEFORE lower in byte order
        assert!(high.as_bytes() < low.as_bytes());
    }
}
