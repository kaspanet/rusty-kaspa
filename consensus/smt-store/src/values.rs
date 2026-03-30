use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// Lane version value.
///
/// Layout: `lane_id(20) | lane_tip_hash(32)` = 52 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct LaneVersion {
    /// Raw 20-byte subnetwork ID. Needed for IBD state reconstruction —
    /// can't reverse `lane_key = H_lane_key(lane_id)`.
    pub lane_id: [u8; 20],
    pub lane_tip_hash: kaspa_hashes::Hash,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;
    use kaspa_smt::store::BranchChildren;

    #[test]
    fn branch_children_round_trip() {
        use zerocopy::IntoBytes;
        let v = BranchChildren { left: Hash::from_bytes([0x11; 32]), right: Hash::from_bytes([0x22; 32]) };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<BranchChildren>());
        let parsed = BranchChildren::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed, v);
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
