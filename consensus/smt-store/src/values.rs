use kaspa_hashes::Hash;

/// Lane version value — just the tip hash.
///
/// The lane_key (SMT position) is already the DB key, and blue_score
/// is encoded in the key as well. No need to store lane_id separately
/// since H_leaf now uses lane_key instead of lane_id.
pub type LaneTipHash = Hash;

#[cfg(test)]
mod tests {
    use kaspa_hashes::Hash;
    use kaspa_smt::store::BranchChildren;
    use zerocopy::FromBytes;

    #[test]
    fn branch_children_round_trip() {
        use zerocopy::IntoBytes;
        let v = BranchChildren { left: Hash::from_bytes([0x11; 32]), right: Hash::from_bytes([0x22; 32]) };
        let bytes = v.as_bytes();
        assert_eq!(bytes.len(), size_of::<BranchChildren>());
        let parsed = BranchChildren::read_from_bytes(bytes).unwrap();
        assert_eq!(parsed, v);
    }
}
