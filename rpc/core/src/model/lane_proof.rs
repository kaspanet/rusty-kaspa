//! Local verification of [`GetSeqCommitLaneProofResponse`] (KIP-21).

use crate::{GetSeqCommitLaneProofResponse, RpcError, RpcHash, RpcLaneEntry};
use kaspa_consensus_core::{hashing::header::hash as header_hash, header::Header};
use kaspa_hashes::SeqCommitActiveNode;
use kaspa_seq_commit::{
    hashing::smt_leaf_hash,
    types::SmtLeafInput,
    verify::{SmtMetadata, SmtVerifyError, verify_smt_metadata},
};
use kaspa_smt::proof::OwnedSmtProof;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LaneProofError {
    #[error("the response has no header")]
    MissingHeader,
    #[error("the response header does not hash to the block hash")]
    HeaderHashMismatch,
    #[error("invalid response header: {0}")]
    InvalidHeader(#[from] RpcError),
    #[error(transparent)]
    Smt(#[from] SmtVerifyError),
}

/// Verifies `response` for `lane_key` against `seq_commit` (the block's `accepted_id_merkle_root`).
///
/// Returns the lane's entry, or `None` for a valid non-inclusion proof. If `parent_seq_commit`
/// is given, the response must name it as its parent.
///
/// The result is only as trustworthy as `seq_commit`: take it from a header the caller has validated.
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

/// Like [`verify_seq_commit_lane_proof`], against `response.header`, which must hash to `block_hash`.
///
/// `block_hash` must come from a trusted source. The header's hash is recomputed: its `hash`
/// field is the node's claim (wRPC passes it through unchecked).
pub fn verify_seq_commit_lane_proof_at_block<'a>(
    response: &'a GetSeqCommitLaneProofResponse,
    lane_key: &RpcHash,
    block_hash: &RpcHash,
    parent_seq_commit: Option<&RpcHash>,
) -> Result<Option<&'a RpcLaneEntry>, LaneProofError> {
    let header = response.header.as_ref().ok_or(LaneProofError::MissingHeader)?;
    if header_hash(&Header::try_from(header)?) != *block_hash {
        return Err(LaneProofError::HeaderHashMismatch);
    }
    Ok(verify_seq_commit_lane_proof(response, lane_key, &header.accepted_id_merkle_root, parent_seq_commit)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{BlueWorkType, header::CompressedParents};
    use kaspa_hashes::{Hash, HasherBase, SeqCommitMerkleBranch};
    use kaspa_seq_commit::{
        hashing::{activity_root_hash, lane_key, seq_state_root},
        types::SeqState,
    };
    use kaspa_smt::tree::SparseMerkleTree;
    use workflow_serializer::prelude::*;

    struct Fixture {
        response: GetSeqCommitLaneProofResponse,
        key: Hash,
        seq_commit: Hash,
        block_hash: Hash,
    }

    /// Three active lanes with tips derived from `salt`; the response for `queried`, with a header committing to it.
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
        let seq_commit = hasher.finalize();
        let header = Header::new_finalized(
            1,
            CompressedParents::try_from(vec![vec![Hash::from_bytes([0xa0; 32])]]).unwrap(),
            Hash::from_bytes([0xa1; 32]),
            seq_commit,
            Hash::from_bytes([0xa2; 32]),
            1_700_000_000_000,
            0x1e7fffff,
            7,
            500,
            BlueWorkType::from_u64(12345),
            400,
            Hash::from_bytes([0xa3; 32]),
        );
        let response = GetSeqCommitLaneProofResponse {
            smt_proof: tree.prove(&key).unwrap().to_bytes(),
            lane: entries.iter().find(|(k, _)| *k == key).map(|(_, entry)| entry.clone()),
            payload_and_ctx_digest,
            parent_seq_commit,
            inactivity_shortcut,
            header: Some((&header).into()),
        };
        Fixture { response, key, seq_commit, block_hash: header.hash }
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
        assert!(verify_seq_commit_lane_proof(&f.response, &f.key, &f.seq_commit, Some(&f.response.parent_seq_commit)).is_ok());
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
    fn at_block_verifies_against_the_header() {
        let f = fixture(2);
        let lane = verify_seq_commit_lane_proof_at_block(&f.response, &f.key, &f.block_hash, None).unwrap().unwrap();
        assert_eq!(lane.tip, Hash::from_bytes([2; 32]));
        let f = fixture(9);
        assert_eq!(verify_seq_commit_lane_proof_at_block(&f.response, &f.key, &f.block_hash, None).unwrap(), None);
    }

    #[test]
    fn at_block_rejects_a_forged_self_consistent_response() {
        let real = fixture(2);
        let mut forged = fixture_with(2, 0x40).response;
        // Consistent on its own: the forged proof matches the forged header's root.
        let forged_root = forged.header.as_ref().unwrap().accepted_id_merkle_root;
        assert!(verify_seq_commit_lane_proof(&forged, &real.key, &forged_root, None).is_ok());
        assert!(matches!(
            verify_seq_commit_lane_proof_at_block(&forged, &real.key, &real.block_hash, None),
            Err(LaneProofError::HeaderHashMismatch)
        ));
        // A header `hash` field claiming the real block is not trusted.
        forged.header.as_mut().unwrap().hash = real.block_hash;
        assert!(matches!(
            verify_seq_commit_lane_proof_at_block(&forged, &real.key, &real.block_hash, None),
            Err(LaneProofError::HeaderHashMismatch)
        ));
    }

    #[test]
    fn at_block_rejects_a_missing_or_altered_header() {
        let f = fixture(2);
        let mut r = f.response.clone();
        r.header = None;
        assert!(matches!(verify_seq_commit_lane_proof_at_block(&r, &f.key, &f.block_hash, None), Err(LaneProofError::MissingHeader)));
        let mut r = f.response.clone();
        r.header.as_mut().unwrap().nonce += 1;
        assert!(matches!(
            verify_seq_commit_lane_proof_at_block(&r, &f.key, &f.block_hash, None),
            Err(LaneProofError::HeaderHashMismatch)
        ));
        assert!(matches!(
            verify_seq_commit_lane_proof_at_block(&f.response, &f.key, &f.block_hash, Some(&Hash::from_bytes([0xee; 32]))),
            Err(LaneProofError::Smt(SmtVerifyError::ParentSeqCommitMismatch { .. }))
        ));
    }

    fn store_v1(response: &GetSeqCommitLaneProofResponse, writer: &mut Vec<u8>) {
        store!(u16, &1, writer).unwrap();
        store!(Vec<u8>, &response.smt_proof, writer).unwrap();
        serialize!(Option<RpcLaneEntry>, &response.lane, writer).unwrap();
        store!(RpcHash, &response.payload_and_ctx_digest, writer).unwrap();
        store!(RpcHash, &response.parent_seq_commit, writer).unwrap();
        store!(RpcHash, &response.inactivity_shortcut, writer).unwrap();
    }

    #[test]
    fn lane_proof_response_v1_reads_without_header() {
        let response = fixture(2).response;
        let mut v1 = Vec::new();
        store_v1(&response, &mut v1);
        let read = <GetSeqCommitLaneProofResponse as Deserializer>::deserialize(&mut v1.as_slice()).unwrap();
        assert_eq!((read.smt_proof, read.lane, read.header.is_none()), (response.smt_proof, response.lane, true));
    }

    /// A reader that only knows version 1.
    struct V1Response(GetSeqCommitLaneProofResponse);

    impl Deserializer for V1Response {
        fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
            let _version = load!(u16, reader)?;
            let smt_proof = load!(Vec<u8>, reader)?;
            let lane = deserialize!(Option<RpcLaneEntry>, reader)?;
            let payload_and_ctx_digest = load!(RpcHash, reader)?;
            let parent_seq_commit = load!(RpcHash, reader)?;
            let inactivity_shortcut = load!(RpcHash, reader)?;
            Ok(Self(GetSeqCommitLaneProofResponse {
                smt_proof,
                lane,
                payload_and_ctx_digest,
                parent_seq_commit,
                inactivity_shortcut,
                header: None,
            }))
        }
    }

    #[test]
    fn lane_proof_response_v2_reads_with_a_v1_reader() {
        let response = fixture(2).response;
        let mut buffer = Vec::new();
        serialize!(GetSeqCommitLaneProofResponse, &response, &mut buffer).unwrap();
        store!(u32, &0x90abcdef, &mut buffer).unwrap();
        let reader = &mut buffer.as_slice();
        let read = deserialize!(V1Response, reader).unwrap().0;
        assert_eq!(load!(u32, reader).unwrap(), 0x90abcdef);
        assert_eq!((read.smt_proof, read.lane), (response.smt_proof, response.lane));
    }
}
