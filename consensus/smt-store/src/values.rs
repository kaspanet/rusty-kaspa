use kaspa_hashes::Hash;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// Branch version value.
///
/// Layout: `left(32) | right(32)` = 64 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct BranchVersion {
    pub left: Hash,
    pub right: Hash,
}

/// Lane version value.
///
/// Layout: `lane_id(20) | lane_tip_hash(32)` = 52 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct LaneVersion {
    pub lane_id: [u8; 20],
    pub lane_tip_hash: Hash,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_version_round_trip() {
        let v = BranchVersion { left: Hash::from_bytes([0x11; 32]), right: Hash::from_bytes([0x22; 32]) };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<BranchVersion>());
        let parsed = BranchVersion::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed.left, Hash::from_bytes([0x11; 32]));
        assert_eq!(parsed.right, Hash::from_bytes([0x22; 32]));
    }

    #[test]
    fn lane_version_round_trip() {
        let v = LaneVersion { lane_id: [0x55; 20], lane_tip_hash: Hash::from_bytes([0x66; 32]) };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<LaneVersion>());
        let parsed = LaneVersion::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed.lane_id, [0x55; 20]);
        assert_eq!(parsed.lane_tip_hash, Hash::from_bytes([0x66; 32]));
    }
}
