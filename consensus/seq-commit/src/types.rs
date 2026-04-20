//! Core types for sequencing commitments.

use kaspa_consensus_core::BlueWorkType;
use kaspa_hashes::Hash;

/// Lane ID — 20-byte subnetwork identifier.
pub type LaneId = [u8; 20];

/// Mergeset context fields hashed into the sequencing commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergesetContext {
    pub timestamp: u64,
    pub daa_score: u64,
    pub blue_score: u64,
}

/// Input for computing the next lane tip hash.
#[derive(Clone, Copy, Debug)]
pub struct LaneTipInput<'a> {
    pub parent_ref: &'a Hash,
    pub lane_key: &'a Hash,
    pub activity_digest: &'a Hash,
    pub context_hash: &'a Hash,
}

/// Input for computing a miner payload leaf hash.
#[derive(Clone, Copy, Debug)]
pub struct MinerPayloadLeafInput<'a> {
    pub block_hash: &'a Hash,
    pub blue_work_bytes: &'a BlueWorkType,
    pub payload: &'a [u8],
}

/// Input for computing an SMT leaf hash for an active lane.
///
/// `leaf_payload = lane_tip_hash(32) || le_u64(blue_score)`.
/// `lane_tip` already commits to `lane_key` via `H_lane_tip`, and the SMT
/// key path commits to `lane_key` as well, so including `lane_key` here
/// would be redundant.
#[derive(Clone, Copy, Debug)]
pub struct SmtLeafInput<'a> {
    pub lane_tip: &'a Hash,
    pub blue_score: u64,
}

/// Components of the sequencing state root.
#[derive(Clone, Copy, Debug)]
pub struct SeqState<'a> {
    pub lanes_root: &'a Hash,
    pub payload_and_ctx_digest: &'a Hash,
}

/// Input for the final sequencing commitment.
#[derive(Clone, Copy, Debug)]
pub struct SeqCommitInput<'a> {
    pub parent_seq_commit: &'a Hash,
    pub state_root: &'a Hash,
}
