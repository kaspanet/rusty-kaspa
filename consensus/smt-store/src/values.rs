use kaspa_hashes::Hash;
use zerocopy::byteorder::big_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// Embedded prev pointer in every version value.
///
/// Layout: `prev_blue_score(8) | prev_hash(32)` = 40 bytes.
/// `prev_blue_score` uses regular big-endian (not reversed) — it's a value
/// field for O(1) linked-list steps, not a sort key.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct PrevPtr {
    pub prev_blue_score: U64,
    pub prev_hash: Hash,
}

impl PrevPtr {
    pub const NULL: Self = Self { prev_blue_score: U64::ZERO, prev_hash: Hash::from_bytes([0; 32]) };

    pub fn new(blue_score: u64, hash: Hash) -> Self {
        Self { prev_blue_score: U64::new(blue_score), prev_hash: hash }
    }

    pub fn is_null(&self) -> bool {
        self.prev_hash == Hash::from_bytes([0; 32])
    }
}

/// Branch version value.
///
/// Layout: `left(32) | right(32) | prev(40)` = 104 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug)]
#[repr(C)]
pub struct BranchVersion {
    pub left: Hash,
    pub right: Hash,
    pub prev: PrevPtr,
}

/// Lane version value.
///
/// Layout: `lane_id(20) | lane_tip_hash(32) | prev(40)` = 92 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug)]
#[repr(C)]
pub struct LaneVersion {
    pub lane_id: [u8; 20],
    pub lane_tip_hash: Hash,
    pub prev: PrevPtr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_version_round_trip() {
        let v = BranchVersion {
            left: Hash::from_bytes([0x11; 32]),
            right: Hash::from_bytes([0x22; 32]),
            prev: PrevPtr::new(42, Hash::from_bytes([0x33; 32])),
        };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<BranchVersion>());
        let parsed = BranchVersion::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed.left, Hash::from_bytes([0x11; 32]));
        assert_eq!(parsed.right, Hash::from_bytes([0x22; 32]));
        assert_eq!(parsed.prev.prev_blue_score.get(), 42);
        assert_eq!(parsed.prev.prev_hash, Hash::from_bytes([0x33; 32]));
    }

    #[test]
    fn lane_version_round_trip() {
        let v = LaneVersion {
            lane_id: [0x55; 20],
            lane_tip_hash: Hash::from_bytes([0x66; 32]),
            prev: PrevPtr::new(7, Hash::from_bytes([0x77; 32])),
        };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<LaneVersion>());
        let parsed = LaneVersion::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed.lane_id, [0x55; 20]);
        assert_eq!(parsed.lane_tip_hash, Hash::from_bytes([0x66; 32]));
        assert_eq!(parsed.prev.prev_blue_score.get(), 7);
    }
}
