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
    use kaspa_smt::store::{CollapsedLeaf, Node};

    #[test]
    fn node_internal_round_trip() {
        let hash = Hash::from_bytes([0x42; 32]);
        let node = Node::Internal(hash);
        let bytes = node.to_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(Node::from_bytes(&bytes), Some(node));
    }

    #[test]
    fn node_collapsed_round_trip() {
        let cl = CollapsedLeaf { lane_key: Hash::from_bytes([0x11; 32]), leaf_hash: Hash::from_bytes([0x22; 32]) };
        let node = Node::Collapsed(cl);
        let bytes = node.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(Node::from_bytes(&bytes), Some(node));
    }
}
