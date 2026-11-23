//! Local verification of [`GetSeqCommitLaneProofResponse`] witnesses (KIP-21).

use crate::{GetSeqCommitLaneProofResponse, RpcHash, RpcLaneEntry};
use kaspa_hashes::SeqCommitActiveNode;
use kaspa_seq_commit::{
    hashing::smt_leaf_hash,
    types::SmtLeafInput,
    verify::{SmtMetadata, SmtVerifyError, verify_smt_metadata},
};
use kaspa_smt::proof::OwnedSmtProof;

impl GetSeqCommitLaneProofResponse {
    /// Verifies this witness for `lane_key` against `seq_commit`, the `accepted_id_merkle_root`
    /// of the header of the block the proof was requested for.
    ///
    /// Returns the lane's entry when the proof shows the lane active at that block, or `None`
    /// when it proves the lane has no entry there. If `parent_seq_commit` is given (the selected
    /// parent header's `accepted_id_merkle_root`), the witness must also name it as its parent.
    ///
    /// The caller is responsible for trusting `seq_commit`, e.g. by validating the header.
    pub fn verify(
        &self,
        lane_key: &RpcHash,
        seq_commit: &RpcHash,
        parent_seq_commit: Option<&RpcHash>,
    ) -> Result<Option<&RpcLaneEntry>, SmtVerifyError> {
        let proof = OwnedSmtProof::from_bytes(&self.smt_proof)?;
        let leaf = self.lane.as_ref().map(|lane| smt_leaf_hash(&SmtLeafInput { lane_tip: &lane.tip, blue_score: lane.blue_score }));
        let lanes_root = proof.compute_root::<SeqCommitActiveNode>(lane_key, leaf)?;
        verify_smt_metadata(
            &SmtMetadata {
                lanes_root: &lanes_root,
                payload_and_ctx_digest: &self.payload_and_ctx_digest,
                parent_seq_commit: &self.parent_seq_commit,
            },
            self.inactivity_shortcut,
            *seq_commit,
            *parent_seq_commit.unwrap_or(&self.parent_seq_commit),
        )?;
        Ok(self.lane.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::{Hash, HasherBase, SeqCommitMerkleBranch};
    use kaspa_seq_commit::{
        hashing::{activity_root_hash, lane_key, seq_state_root},
        types::SeqState,
    };
    use kaspa_smt::tree::SparseMerkleTree;

    struct Fixture {
        response: GetSeqCommitLaneProofResponse,
        key: Hash,
        seq_commit: Hash,
    }

    /// A lanes tree with three active lanes, a witness for `queried` and the matching `seq_commit`.
    fn fixture(queried: u8) -> Fixture {
        let entries: Vec<(Hash, RpcLaneEntry)> = [1u8, 2, 3]
            .iter()
            .map(|&b| (lane_key(&[b; 20]), RpcLaneEntry { tip: Hash::from_bytes([b; 32]), blue_score: 1000 + b as u64 }))
            .collect();
        let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
        for (key, entry) in &entries {
            tree.insert(*key, smt_leaf_hash(&SmtLeafInput { lane_tip: &entry.tip, blue_score: entry.blue_score }));
        }
        let key = lane_key(&[queried; 20]);
        let payload_and_ctx_digest = Hash::from_bytes([0xd0; 32]);
        let parent_seq_commit = Hash::from_bytes([0xd1; 32]);
        let inactivity_shortcut = Hash::from_bytes([0xd2; 32]);
        let activity_root = activity_root_hash(&inactivity_shortcut, &tree.root());
        let state_root = seq_state_root(&SeqState { activity_root: &activity_root, payload_and_ctx_digest: &payload_and_ctx_digest });
        let mut hasher = SeqCommitMerkleBranch::new();
        hasher.update(parent_seq_commit).update(state_root);
        let response = GetSeqCommitLaneProofResponse {
            smt_proof: tree.prove(&key).unwrap().to_bytes(),
            lane: entries.iter().find(|(k, _)| *k == key).map(|(_, entry)| entry.clone()),
            payload_and_ctx_digest,
            parent_seq_commit,
            inactivity_shortcut,
        };
        Fixture { response, key, seq_commit: hasher.finalize() }
    }

    #[test]
    fn active_lane_verifies() {
        let f = fixture(2);
        let lane = f.response.verify(&f.key, &f.seq_commit, None).unwrap().unwrap();
        assert_eq!(lane.tip, Hash::from_bytes([2; 32]));
        assert_eq!(lane.blue_score, 1002);
        assert!(f.response.verify(&f.key, &f.seq_commit, Some(&f.response.parent_seq_commit)).is_ok());
    }

    #[test]
    fn absent_lane_verifies_as_inactive() {
        let f = fixture(9);
        assert!(f.response.lane.is_none());
        assert_eq!(f.response.verify(&f.key, &f.seq_commit, None).unwrap(), None);
    }

    #[test]
    fn altered_witness_is_rejected() {
        let f = fixture(2);
        let reject = |response: &GetSeqCommitLaneProofResponse| assert!(response.verify(&f.key, &f.seq_commit, None).is_err());

        let mut r = f.response.clone();
        r.lane.as_mut().unwrap().tip = Hash::from_bytes([7; 32]);
        reject(&r);

        let mut r = f.response.clone();
        r.lane.as_mut().unwrap().blue_score += 1;
        reject(&r);

        // Hiding an active lane does not pass as a non-inclusion proof.
        let mut r = f.response.clone();
        r.lane = None;
        reject(&r);

        let mut r = f.response.clone();
        r.payload_and_ctx_digest = Hash::from_bytes([0xee; 32]);
        reject(&r);

        let mut r = f.response.clone();
        r.inactivity_shortcut = Hash::from_bytes([0xee; 32]);
        reject(&r);

        let mut r = f.response.clone();
        let last = r.smt_proof.len() - 1;
        r.smt_proof[last] ^= 1;
        reject(&r);

        let mut r = f.response.clone();
        r.smt_proof.truncate(10);
        reject(&r);
    }

    #[test]
    fn wrong_block_or_lane_is_rejected() {
        let f = fixture(2);
        assert!(f.response.verify(&f.key, &Hash::from_bytes([0xee; 32]), None).is_err());
        assert!(f.response.verify(&lane_key(&[3; 20]), &f.seq_commit, None).is_err());
        assert!(matches!(
            f.response.verify(&f.key, &f.seq_commit, Some(&Hash::from_bytes([0xee; 32]))),
            Err(SmtVerifyError::ParentSeqCommitMismatch { .. })
        ));
    }
}
