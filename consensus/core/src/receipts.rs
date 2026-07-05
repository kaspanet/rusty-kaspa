use std::ops::Deref;

use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_merkle::MerkleWitness;
use kaspa_seq_commit::types::{LaneTipInput, SeqCommitInput, SeqState, SmtLeafInput};
use kaspa_smt::proof::OwnedSmtProof;

// Hash functions for sequencing commitments.
//
// Commitment tree shape:
//
// ```text
// SeqCommit(B)
// └── H_seq
//     ├── parent_seq_commit = SeqCommit(selected_parent(B))
//     └── state_root
//         └── H_seq
//             ├── activity_root
//             │   └── H_activity_root
//             │       ├── inactivity_shortcut
//             │       └── lanes_root
//             │           └── SMT root over active lanes
//             │
//             └── payload_and_ctx_digest
//                 └── H_seq
//                     ├── context_hash
//                     └── payload_root
// ```
//

/// Receipt data needed to verify a transaction from the accepting block to the posterity block.
///
/// The receipt stores the accepting lane-tip preimage, proofs for the accepting lane's activity and
/// SMT membership, and the lane-tip updates needed to walk that lane forward to the posterity block.
#[derive(Clone)]
pub struct TxReceipt {
    pub posterity_hash: Hash,
    pub tx_context: TxContext,
    pub accepting_blk_context: AcceptingBlkContext,
    pub lane_tip_updates_to_posterity: Vec<LaneTipUpdateContext>,
    posterity_context: PosterityReceiptContext,
    lane_key: Hash,
}

#[derive(Clone)]
pub struct TxContext {
    pub tracked_tx_id: Hash,
    pub tx_version: u16,
    pub tx_merge_idx: u32,
}

#[derive(Clone)]
pub struct LaneTipUpdateContext {
    pub activity_digest: Hash,
    pub context_hash: Hash,
}

/// Accepting-block context needed to reconstruct the accepting lane tip and verify lane inclusion.
#[derive(Clone)]
pub struct AcceptingBlkContext {
    pub tx_acceptance_proof: MerkleWitness,
    pub context_hash: Hash,
    pub parent_ref: Hash,
}

/// Posterity-side context used to reconstruct the posterity sequencing commitment.
#[derive(Clone)]
pub struct PosterityReceiptContext {
    pub lane_in_active_lanes_proof: OwnedSmtProof,
    pub lane_blue_score: u64,
    pub inactivity_shortcut: Hash,
    pub payload_and_ctx_digest: Hash,
    pub parent_sqc: Hash,
}

impl Deref for TxReceipt {
    type Target = TxContext;

    fn deref(&self) -> &Self::Target {
        &self.tx_context
    }
}

impl TxReceipt {
    pub fn new(
        posterity_hash: Hash,
        tx_context: TxContext,
        accepting_blk_context: AcceptingBlkContext,
        lane_tip_updates_to_posterity: Vec<LaneTipUpdateContext>,
        lane_key: Hash,
        posterity_context: PosterityReceiptContext,
    ) -> Self {
        Self { posterity_hash, tx_context, accepting_blk_context, lane_tip_updates_to_posterity, posterity_context, lane_key }
    }

    fn lane_tip_next(&self, parent_ref: &Hash, activity_digest: &Hash, context_hash: &Hash) -> Hash {
        kaspa_seq_commit::hashing::lane_tip_next(&LaneTipInput { parent_ref, lane_key: &self.lane_key, activity_digest, context_hash })
    }

    pub fn activity_leaf(&self) -> Hash {
        kaspa_seq_commit::hashing::activity_leaf(&self.tracked_tx_id, self.tx_version, self.tx_merge_idx)
    }

    pub fn compute_accepting_activity_digest_in_lane(&self) -> Hash {
        kaspa_merkle::compute_merkle_witness_root(&self.accepting_blk_context.tx_acceptance_proof, self.activity_leaf())
    }

    pub fn accepting_lane_tip(&self, activity_digest: Hash) -> Hash {
        self.lane_tip_next(&self.accepting_blk_context.parent_ref, &activity_digest, &self.accepting_blk_context.context_hash)
    }

    pub fn posterity_lane_tip(&self, accepting_blk_lane_tip: Hash) -> Hash {
        let mut current_tip = accepting_blk_lane_tip;

        for update in self.lane_tip_updates_to_posterity.iter() {
            current_tip = self.lane_tip_next(&current_tip, &update.activity_digest, &update.context_hash);
        }

        current_tip
    }

    pub fn verify_receipt(&self, posterity_sqc: Hash) -> bool {
        // 1) Derive the alleged activity digest of the accepting block via tx acceptance proof.
        let activity_digest = self.compute_accepting_activity_digest_in_lane();

        // 2) Derive the accepting lane tip from the accepting-block context.
        let accepting_blk_lane_tip = self.accepting_lane_tip(activity_digest);

        // 3) Walk the lane-tip chain forward to the posterity block.
        let posterity_lane_tip = self.posterity_lane_tip(accepting_blk_lane_tip);

        // 4) Reconstruct the posterity SQC from the posterity lane tip.
        matches!(self.posterity_sqc(posterity_lane_tip), Some(sqc) if sqc == posterity_sqc)
    }

    /// Reconstruct the posterity sequencing commitment from the posterity lane tip and posterity context.
    pub fn posterity_sqc(&self, posterity_lane_tip: Hash) -> Option<Hash> {
        let posterity_lane_leaf = kaspa_seq_commit::hashing::smt_leaf_hash(&SmtLeafInput {
            lane_tip: &posterity_lane_tip,
            blue_score: self.posterity_context.lane_blue_score,
        });

        let posterity_active_lanes_root = self
            .posterity_context
            .lane_in_active_lanes_proof
            .as_proof()
            .compute_root::<SeqCommitActiveNode>(&self.lane_key, Some(posterity_lane_leaf))
            .ok()?;

        // Recompute the posterity block sequencing commitment from the receipt internals.
        let activity_root =
            kaspa_seq_commit::hashing::activity_root_hash(&self.posterity_context.inactivity_shortcut, &posterity_active_lanes_root);
        let state_root = kaspa_seq_commit::hashing::seq_state_root(&SeqState {
            activity_root: &activity_root,
            payload_and_ctx_digest: &self.posterity_context.payload_and_ctx_digest,
        });
        let sequencing_commitment = kaspa_seq_commit::hashing::seq_commit(&SeqCommitInput {
            parent_seq_commit: &self.posterity_context.parent_sqc,
            state_root: &state_root,
        });

        Some(sequencing_commitment)
    }
}
