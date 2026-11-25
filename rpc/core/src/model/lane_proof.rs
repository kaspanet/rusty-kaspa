//! Local verification of [`GetSeqCommitLaneProofResponse`] (KIP-21).

use crate::{GetSeqCommitLaneProofResponse, RpcHash, RpcLaneEntry};
use kaspa_hashes::SeqCommitActiveNode;
use kaspa_seq_commit::{
    hashing::smt_leaf_hash,
    types::SmtLeafInput,
    verify::{SmtMetadata, SmtVerifyError, verify_smt_metadata},
};
use kaspa_smt::proof::OwnedSmtProof;

/// Verifies `response` for `lane_key` against `seq_commit`.
///
/// `seq_commit` is the `accepted_id_merkle_root` of the queried block, from the chain the caller
/// follows itself, never from the response. This checks the lane state against that commitment;
/// it does not show that the block is on the selected chain.
///
/// Returns the lane's entry, or `None` for a valid non-inclusion proof. If `parent_seq_commit`
/// (the selected parent's) is given, the response must name it as its parent.
pub fn verify_seq_commit_lane_proof<'a>(
    response: &'a GetSeqCommitLaneProofResponse,
    lane_key: &RpcHash,
    seq_commit: &RpcHash,
    parent_seq_commit: Option<&RpcHash>,
) -> Result<Option<&'a RpcLaneEntry>, SmtVerifyError> {
    let proof = OwnedSmtProof::from_bytes(&response.smt_proof)?;
    let leaf = response.lane.as_ref().map(|lane| smt_leaf_hash(&SmtLeafInput { lane_tip: &lane.tip, blue_score: lane.blue_score }));
    let lanes_root = proof.compute_root::<SeqCommitActiveNode>(lane_key, leaf)?;
    verify_smt_metadata(
        &SmtMetadata {
            lanes_root: &lanes_root,
            payload_and_ctx_digest: &response.payload_and_ctx_digest,
            parent_seq_commit: &response.parent_seq_commit,
        },
        response.inactivity_shortcut,
        *seq_commit,
        *parent_seq_commit.unwrap_or(&response.parent_seq_commit),
    )?;
    Ok(response.lane.as_ref())
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

    /// Three active lanes with tips derived from `salt`; the response for `queried` and the seq commit it verifies against.
    fn fixture_with(queried: u8, salt: u8) -> Fixture {
        let entries: Vec<(Hash, RpcLaneEntry)> = [1u8, 2, 3]
            .iter()
            .map(|&b| (lane_key(&[b; 20]), RpcLaneEntry { tip: Hash::from_bytes([b ^ salt; 32]), blue_score: 1000 + b as u64 }))
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

    fn fixture(queried: u8) -> Fixture {
        fixture_with(queried, 0)
    }

    fn verify(f: &Fixture, response: &GetSeqCommitLaneProofResponse) -> Result<Option<RpcLaneEntry>, SmtVerifyError> {
        verify_seq_commit_lane_proof(response, &f.key, &f.seq_commit, None).map(|lane| lane.cloned())
    }

    #[test]
    fn active_lane_verifies() {
        let f = fixture(2);
        let lane = verify(&f, &f.response).unwrap().unwrap();
        assert_eq!(lane.tip, Hash::from_bytes([2; 32]));
        assert_eq!(lane.blue_score, 1002);
        assert!(verify_seq_commit_lane_proof(&f.response, &f.key, &f.seq_commit, Some(&Hash::from_bytes([0xd1; 32]))).is_ok());
    }

    #[test]
    fn absent_lane_verifies_as_inactive() {
        let f = fixture(9);
        assert!(f.response.lane.is_none());
        assert_eq!(verify(&f, &f.response).unwrap(), None);
    }

    #[test]
    fn altered_response_is_rejected() {
        let f = fixture(2);
        let reject = |response: &GetSeqCommitLaneProofResponse| assert!(verify(&f, response).is_err());

        let mut r = f.response.clone();
        r.lane.as_mut().unwrap().tip = Hash::from_bytes([7; 32]);
        reject(&r);

        let mut r = f.response.clone();
        r.lane.as_mut().unwrap().blue_score += 1;
        reject(&r);

        // An active lane cannot pass as absent.
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
        r.parent_seq_commit = Hash::from_bytes([0xee; 32]);
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
        assert!(verify_seq_commit_lane_proof(&f.response, &f.key, &Hash::from_bytes([0xee; 32]), None).is_err());
        assert!(verify_seq_commit_lane_proof(&f.response, &lane_key(&[3; 20]), &f.seq_commit, None).is_err());
        assert!(matches!(
            verify_seq_commit_lane_proof(&f.response, &f.key, &f.seq_commit, Some(&Hash::from_bytes([0xee; 32]))),
            Err(SmtVerifyError::ParentSeqCommitMismatch { .. })
        ));
    }

    #[test]
    fn a_self_consistent_response_for_another_commitment_is_rejected() {
        let real = fixture(2);
        let forged = fixture_with(2, 0x40);
        // The forged response is valid against its own commitment, but not against the one the caller tracks.
        assert!(verify_seq_commit_lane_proof(&forged.response, &real.key, &forged.seq_commit, None).is_ok());
        assert!(matches!(
            verify_seq_commit_lane_proof(&forged.response, &real.key, &real.seq_commit, None),
            Err(SmtVerifyError::SeqCommitMismatch { .. })
        ));
    }
}
