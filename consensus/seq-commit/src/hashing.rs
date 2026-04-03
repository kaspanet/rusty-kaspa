//! Hash functions for sequencing commitments.

use kaspa_hashes::{
    Hash, HasherBase, SeqCommitActiveLeaf, SeqCommitActivityLeaf, SeqCommitLaneKey, SeqCommitLaneTip, SeqCommitMergesetContext,
    SeqCommitMerkleBranch, SeqCommitMinerPayload, SeqCommitMinerPayloadLeaf, SeqCommitTxDigest,
};
use kaspa_merkle::{StreamingMerkleBuilder, calc_merkle_root_with_hasher};

use crate::types::{LaneId, LaneTipInput, MergesetContext, MinerPayloadLeafInput, SeqCommitInput, SeqState, SmtLeafInput};

/// Derive the timestamp used in `MergesetContextHash` for a given block.
///
/// Uses the selected parent's timestamp — a consensus-agreed deterministic
/// value that doesn't depend on miner input. This ensures the seq_commit
/// can be computed for the virtual block without knowing the final header
/// timestamp.
#[inline]
pub fn seq_commit_timestamp(selected_parent_timestamp: u64) -> u64 {
    selected_parent_timestamp
}

/// Compute the tx digest for sequencing: `H_tx_digest(tx_id || le_u16(version))`.
#[inline]
pub fn seq_commit_tx_digest(tx_id: &Hash, version: u16) -> Hash {
    let mut hasher = SeqCommitTxDigest::new();
    hasher.update(tx_id).update(version.to_le_bytes());
    hasher.finalize()
}

/// Compute the SMT key for a lane: `H_lane_key(lane_id)`.
#[inline]
pub fn lane_key(lane_id: &LaneId) -> Hash {
    let mut hasher = SeqCommitLaneKey::new();
    hasher.update(lane_id);
    hasher.finalize()
}

/// Compute an activity leaf: `H_activity_leaf(tx_digest || le_u32(merge_idx))`.
#[inline]
pub fn activity_leaf(tx_digest: &Hash, merge_idx: u32) -> Hash {
    let mut hasher = SeqCommitActivityLeaf::new();
    hasher.update(tx_digest).update(merge_idx.to_le_bytes());
    hasher.finalize()
}

/// Compute the activity digest for a lane's transactions in a mergeset block.
///
/// Merkle root over the activity leaves using `H_seq` (SeqCommitMerkleBranch).
/// For a single leaf, hashes `H_seq(leaf || ZERO_HASH)`.
#[inline]
pub fn activity_digest_lane(leaves: impl ExactSizeIterator<Item = Hash>) -> Hash {
    calc_merkle_root_with_hasher::<SeqCommitMerkleBranch, true>(leaves)
}

/// Streaming activity digest builder for no-heap environments (ZK guests).
///
/// Produces the same root as [`activity_digest_lane`] but processes leaves
/// one at a time without heap allocation.
pub type ActivityDigestBuilder = StreamingMerkleBuilder<SeqCommitMerkleBranch>;

/// Compute the next lane tip: `H_lane_tip(parent_ref || lane_key || activity_digest || context_hash)`.
#[inline]
pub fn lane_tip_next(input: &LaneTipInput) -> Hash {
    let mut hasher = SeqCommitLaneTip::new();
    hasher.update(input.parent_ref).update(input.lane_key).update(input.activity_digest).update(input.context_hash);
    hasher.finalize()
}

/// Compute the mergeset context hash: `H_mergeset_context(le_u64(timestamp) || le_u64(daa_score) || le_u64(blue_score))`.
#[inline]
pub fn mergeset_context_hash(ctx: &MergesetContext) -> Hash {
    let mut hasher = SeqCommitMergesetContext::new();
    hasher.update(ctx.timestamp.to_le_bytes()).update(ctx.daa_score.to_le_bytes()).update(ctx.blue_score.to_le_bytes());
    hasher.finalize()
}

/// Compute the miner payload hash: `H_miner_payload(payload_bytes)`.
#[inline]
pub fn miner_payload_hash(payload: &[u8]) -> Hash {
    let mut hasher = SeqCommitMinerPayload::new();
    hasher.update(payload);
    hasher.finalize()
}

/// Compute a miner payload leaf: `H_miner_payload_leaf(block_hash || blue_work_bytes || H_miner_payload(payload))`.
#[inline]
pub fn miner_payload_leaf(input: &MinerPayloadLeafInput<'_>) -> Hash {
    let payload_h = miner_payload_hash(input.payload);
    let mut hasher = SeqCommitMinerPayloadLeaf::new();
    hasher.update(input.block_hash).update(input.blue_work_bytes).update(payload_h);
    hasher.finalize()
}

/// Compute the miner payload root from payload leaves.
///
/// Merkle root using `H_seq` (SeqCommitMerkleBranch).
/// For a single leaf, hashes `H_seq(leaf || ZERO_HASH)`.
#[inline]
pub fn miner_payload_root(leaves: impl ExactSizeIterator<Item = Hash>) -> Hash {
    calc_merkle_root_with_hasher::<SeqCommitMerkleBranch, true>(leaves)
}

/// Compute the SMT leaf hash for an active lane:
/// `H_active_leaf(lane_key(32) || lane_tip_hash(32) || le_u64(blue_score))`.
#[inline]
pub fn smt_leaf_hash(input: &SmtLeafInput<'_>) -> Hash {
    let mut hasher = SeqCommitActiveLeaf::new();
    hasher.update(input.lane_key).update(input.lane_tip).update(input.blue_score.to_le_bytes());
    hasher.finalize()
}

/// Compute the payload digest: `H_seq(context_hash, payload_root)`.
///
/// Combines the mergeset context and miner payload into a single hash
/// that can be stored and reused without access to block transactions.
#[inline]
pub fn payload_and_context_digest(context_hash: &Hash, payload_root: &Hash) -> Hash {
    let mut hasher = SeqCommitMerkleBranch::new();
    hasher.update(context_hash).update(payload_root);
    hasher.finalize()
}

/// Compute the seq-state root: `H_seq(lanes_root, payload_and_ctx_digest)`.
#[inline]
pub fn seq_state_root(state: &SeqState<'_>) -> Hash {
    let mut hasher = SeqCommitMerkleBranch::new();
    hasher.update(state.lanes_root).update(state.payload_and_ctx_digest);
    hasher.finalize()
}

/// Compute the final sequencing commitment: `H_seq(parent_seq_commit, state_root)`.
#[inline]
pub fn seq_commit(input: &SeqCommitInput<'_>) -> Hash {
    let mut hasher = SeqCommitMerkleBranch::new();
    hasher.update(input.parent_seq_commit).update(input.state_root);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::ZERO_HASH;

    fn h(b: u8) -> Hash {
        let mut bytes = [0u8; 32];
        bytes[0] = b;
        Hash::from_bytes(bytes)
    }

    #[test]
    fn test_lane_key_golden() {
        let expected = Hash::from_bytes([
            0x57, 0xc7, 0xe5, 0x2c, 0x76, 0x02, 0xb3, 0x66, 0xb3, 0xf6, 0x62, 0xad, 0xdc, 0x36, 0x12, 0x96, 0x77, 0xd4, 0x84, 0x4b,
            0x84, 0x04, 0x68, 0xcc, 0xaa, 0x96, 0x31, 0x10, 0x6b, 0xea, 0x88, 0x97,
        ]);
        assert_eq!(lane_key(&[0x42; 20]), expected);
    }

    #[test]
    fn test_lane_key_different_ids() {
        assert_ne!(lane_key(&[0x01; 20]), lane_key(&[0x02; 20]));
    }

    #[test]
    fn test_activity_leaf_golden() {
        let expected = Hash::from_bytes([
            0x34, 0x76, 0x1d, 0xfa, 0x0a, 0xb5, 0x9c, 0x41, 0x37, 0x22, 0x3a, 0x8c, 0xce, 0xd1, 0x72, 0x61, 0x60, 0x6b, 0x2e, 0xbf,
            0x98, 0x3c, 0xed, 0x15, 0xb6, 0x8a, 0x57, 0xd7, 0xb6, 0x30, 0xff, 0x94,
        ]);
        assert_eq!(activity_leaf(&h(1), 0), expected);
    }

    #[test]
    fn test_activity_leaf_different_indices() {
        assert_ne!(activity_leaf(&h(1), 0), activity_leaf(&h(1), 1));
    }

    #[test]
    fn test_activity_leaf_different_digests() {
        assert_ne!(activity_leaf(&h(1), 0), activity_leaf(&h(2), 0));
    }

    #[test]
    fn test_activity_digest_lane_empty() {
        assert_eq!(activity_digest_lane(core::iter::empty()), ZERO_HASH);
    }

    #[test]
    fn test_activity_digest_lane_single() {
        let leaf = h(1);
        let root = activity_digest_lane(core::iter::once(leaf));
        let expected = {
            let mut hasher = SeqCommitMerkleBranch::new();
            hasher.update(leaf).update(ZERO_HASH);
            hasher.finalize()
        };
        assert_eq!(root, expected);
    }

    #[test]
    fn test_activity_digest_lane_two() {
        let a = h(1);
        let b = h(2);
        let root = activity_digest_lane([a, b].into_iter());
        let expected = {
            let mut hasher = SeqCommitMerkleBranch::new();
            hasher.update(a).update(b);
            hasher.finalize()
        };
        assert_eq!(root, expected);
    }

    #[test]
    fn test_lane_tip_next_golden() {
        let expected = Hash::from_bytes([
            0x38, 0x79, 0x35, 0x76, 0xd5, 0x45, 0xd2, 0x95, 0x2d, 0xf3, 0x1b, 0x7c, 0x57, 0x19, 0x3a, 0x49, 0x2f, 0x9c, 0x5b, 0x8c,
            0x09, 0xdd, 0xae, 0xb2, 0x27, 0x4e, 0x4e, 0x23, 0xed, 0xbc, 0x3f, 0x43,
        ]);
        let lk = lane_key(&[0x55; 20]);
        let input = LaneTipInput { parent_ref: &h(1), lane_key: &lk, activity_digest: &h(2), context_hash: &h(3) };
        assert_eq!(lane_tip_next(&input), expected);
    }

    #[test]
    fn test_lane_tip_next_different_parents() {
        let lk = lane_key(&[0x55; 20]);
        let a = LaneTipInput { parent_ref: &h(1), lane_key: &lk, activity_digest: &h(2), context_hash: &h(3) };
        let b = LaneTipInput { parent_ref: &h(10), lane_key: &lk, activity_digest: &h(2), context_hash: &h(3) };
        assert_ne!(lane_tip_next(&a), lane_tip_next(&b));
    }

    #[test]
    fn test_mergeset_context_hash_golden() {
        let expected = Hash::from_bytes([
            0x20, 0xd8, 0xeb, 0x88, 0xd6, 0x6b, 0xef, 0x8f, 0xee, 0xf3, 0x60, 0x24, 0x3e, 0xb5, 0x10, 0xee, 0x53, 0x3b, 0x4c, 0xe0,
            0x07, 0x43, 0x2c, 0x29, 0xb4, 0xf5, 0x58, 0xb0, 0xdb, 0xf4, 0x75, 0x91,
        ]);
        assert_eq!(mergeset_context_hash(&MergesetContext { timestamp: 1000, daa_score: 500, blue_score: 250 }), expected);
    }

    #[test]
    fn test_mergeset_context_hash_different_inputs() {
        let a = MergesetContext { timestamp: 1000, daa_score: 500, blue_score: 250 };
        let b = MergesetContext { timestamp: 1001, daa_score: 500, blue_score: 250 };
        let c = MergesetContext { timestamp: 1000, daa_score: 501, blue_score: 250 };
        let d = MergesetContext { timestamp: 1000, daa_score: 500, blue_score: 251 };
        let ha = mergeset_context_hash(&a);
        assert_ne!(ha, mergeset_context_hash(&b));
        assert_ne!(ha, mergeset_context_hash(&c));
        assert_ne!(ha, mergeset_context_hash(&d));
    }

    #[test]
    fn test_miner_payload_hash_golden() {
        let expected = Hash::from_bytes([
            0x8e, 0xfa, 0x8d, 0xf0, 0xd5, 0x4d, 0xee, 0xca, 0x36, 0xf5, 0x7b, 0xf9, 0x1a, 0x01, 0x73, 0x1c, 0xc2, 0x05, 0x3f, 0x09,
            0x61, 0x81, 0xaf, 0x12, 0x64, 0xc4, 0x79, 0x9b, 0x0d, 0x92, 0x88, 0x98,
        ]);
        assert_eq!(miner_payload_hash(b"hello miner"), expected);
    }

    #[test]
    fn test_miner_payload_hash_different_payloads() {
        assert_ne!(miner_payload_hash(b"abc"), miner_payload_hash(b"def"));
    }

    #[test]
    fn test_miner_payload_leaf_golden() {
        let expected = Hash::from_bytes([
            0xca, 0x36, 0xc3, 0x3d, 0xb7, 0xba, 0xc7, 0x1a, 0x7e, 0x7a, 0x9d, 0x21, 0x0f, 0x5b, 0x6c, 0x2b, 0xdb, 0x01, 0x2c, 0xc9,
            0xcf, 0xa9, 0x71, 0xfc, 0x7d, 0x96, 0x90, 0x49, 0xe4, 0x75, 0x25, 0x0b,
        ]);
        let input = MinerPayloadLeafInput { block_hash: &h(1), blue_work_bytes: &[0x01, 0x23], payload: b"coinbase data" };
        assert_eq!(miner_payload_leaf(&input), expected);
    }

    #[test]
    fn test_miner_payload_leaf_includes_all_inputs() {
        let base = miner_payload_leaf(&MinerPayloadLeafInput { block_hash: &h(1), blue_work_bytes: &[0x01], payload: b"data" });
        assert_ne!(base, miner_payload_leaf(&MinerPayloadLeafInput { block_hash: &h(2), blue_work_bytes: &[0x01], payload: b"data" }));
        assert_ne!(base, miner_payload_leaf(&MinerPayloadLeafInput { block_hash: &h(1), blue_work_bytes: &[0x02], payload: b"data" }));
        assert_ne!(
            base,
            miner_payload_leaf(&MinerPayloadLeafInput { block_hash: &h(1), blue_work_bytes: &[0x01], payload: b"other" })
        );
    }

    #[test]
    fn test_miner_payload_root_empty() {
        assert_eq!(miner_payload_root(core::iter::empty()), ZERO_HASH);
    }

    #[test]
    fn test_miner_payload_root_single() {
        let leaf = h(1);
        let root = miner_payload_root(core::iter::once(leaf));
        let expected = {
            let mut hasher = SeqCommitMerkleBranch::new();
            hasher.update(leaf).update(ZERO_HASH);
            hasher.finalize()
        };
        assert_eq!(root, expected);
    }

    #[test]
    fn test_smt_leaf_hash_golden() {
        let result = smt_leaf_hash(&SmtLeafInput { lane_key: &h(1), lane_tip: &h(2), blue_score: 100 });
        // Verify determinism
        let result2 = smt_leaf_hash(&SmtLeafInput { lane_key: &h(1), lane_tip: &h(2), blue_score: 100 });
        assert_eq!(result, result2);
        assert_ne!(result, ZERO_HASH);
    }

    #[test]
    fn test_smt_leaf_hash_different_inputs() {
        let base = smt_leaf_hash(&SmtLeafInput { lane_key: &h(1), lane_tip: &h(2), blue_score: 100 });
        assert_ne!(base, smt_leaf_hash(&SmtLeafInput { lane_key: &h(10), lane_tip: &h(2), blue_score: 100 }));
        assert_ne!(base, smt_leaf_hash(&SmtLeafInput { lane_key: &h(1), lane_tip: &h(20), blue_score: 100 }));
        assert_ne!(base, smt_leaf_hash(&SmtLeafInput { lane_key: &h(1), lane_tip: &h(2), blue_score: 200 }));
    }

    #[test]
    fn test_seq_state_root_golden() {
        let expected = Hash::from_bytes([
            0x20, 0xd1, 0x35, 0x77, 0x5a, 0x39, 0xbe, 0x49, 0xfe, 0x34, 0x70, 0x9a, 0x55, 0xb0, 0xc7, 0xeb, 0x39, 0x05, 0xf5, 0xc9,
            0xbe, 0x27, 0xd1, 0xdc, 0x11, 0x50, 0xb8, 0xaf, 0x23, 0x9e, 0x56, 0xd9,
        ]);
        let (lr, ch, pr) = (h(1), h(2), h(3));
        let pd = payload_and_context_digest(&ch, &pr);
        assert_eq!(seq_state_root(&SeqState { lanes_root: &lr, payload_and_ctx_digest: &pd }), expected);
    }

    #[test]
    fn test_seq_state_root_structure() {
        let (lr, ch, pr) = (h(1), h(2), h(3));
        let pd = payload_and_context_digest(&ch, &pr);
        let state = SeqState { lanes_root: &lr, payload_and_ctx_digest: &pd };
        let expected = {
            let mut hasher = SeqCommitMerkleBranch::new();
            hasher.update(state.lanes_root).update(state.payload_and_ctx_digest);
            hasher.finalize()
        };
        assert_eq!(seq_state_root(&state), expected);
    }

    #[test]
    fn test_seq_commit_golden() {
        let expected = Hash::from_bytes([
            0x7e, 0x9e, 0x79, 0x76, 0x99, 0x56, 0x7a, 0xb5, 0xcb, 0xcc, 0xc5, 0xa7, 0xe7, 0x20, 0xc6, 0x27, 0x09, 0x9d, 0xf8, 0x31,
            0x86, 0xe1, 0xd1, 0xe1, 0xca, 0xe8, 0x8d, 0x86, 0x46, 0xc0, 0x63, 0xdf,
        ]);
        let (p, s) = (h(1), h(2));
        assert_eq!(seq_commit(&SeqCommitInput { parent_seq_commit: &p, state_root: &s }), expected);
    }

    #[test]
    fn test_seq_commit_structure() {
        let (p, s) = (h(1), h(2));
        let input = SeqCommitInput { parent_seq_commit: &p, state_root: &s };
        let expected = {
            let mut hasher = SeqCommitMerkleBranch::new();
            hasher.update(input.parent_seq_commit).update(input.state_root);
            hasher.finalize()
        };
        assert_eq!(seq_commit(&input), expected);
    }

    #[test]
    fn test_seq_commit_different_inputs() {
        let (h1, h2, h3) = (h(1), h(2), h(3));
        let a = seq_commit(&SeqCommitInput { parent_seq_commit: &h1, state_root: &h2 });
        let b = seq_commit(&SeqCommitInput { parent_seq_commit: &h2, state_root: &h1 });
        let c = seq_commit(&SeqCommitInput { parent_seq_commit: &h1, state_root: &h3 });
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_end_to_end_commitment() {
        let lane_id: LaneId = [0x01; 20];
        let tx_digest = h(42);
        let parent_commit = h(99);

        let al = activity_leaf(&tx_digest, 0);
        let ad = activity_digest_lane(core::iter::once(al));
        let ctx = mergeset_context_hash(&MergesetContext { timestamp: 1_700_000_000, daa_score: 100_000, blue_score: 50_000 });

        let lk = lane_key(&lane_id);
        let tip = lane_tip_next(&LaneTipInput { parent_ref: &parent_commit, lane_key: &lk, activity_digest: &ad, context_hash: &ctx });
        let smt_leaf = smt_leaf_hash(&SmtLeafInput { lane_key: &lk, lane_tip: &tip, blue_score: 50_000 });

        let h1 = h(1);
        let mpl = miner_payload_leaf(&MinerPayloadLeafInput { block_hash: &h1, blue_work_bytes: &[0x01, 0x00], payload: b"coinbase" });
        let mpr = miner_payload_root(core::iter::once(mpl));

        let pd = payload_and_context_digest(&ctx, &mpr);
        let state_root = seq_state_root(&SeqState { lanes_root: &smt_leaf, payload_and_ctx_digest: &pd });
        let commitment = seq_commit(&SeqCommitInput { parent_seq_commit: &parent_commit, state_root: &state_root });
        assert_ne!(commitment, parent_commit);
    }
}
