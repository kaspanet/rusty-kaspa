//! Core types for sequencing commitments.

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
    pub lane_id: &'a LaneId,
    pub activity_digest: &'a Hash,
    pub context_hash: &'a Hash,
}

/// Input for computing a miner payload leaf hash.
#[derive(Clone, Copy, Debug)]
pub struct MinerPayloadLeafInput<'a> {
    pub block_hash: &'a Hash,
    pub blue_work_bytes: &'a [u8],
    pub payload: &'a [u8],
}

/// Input for computing an SMT leaf hash for an active lane.
///
/// Per KIP-21 §6.2: `leaf_payload = lane_id_bytes(20) || lane_tip_hash(32) || le_u64(blue_score)`.
#[derive(Clone, Copy, Debug)]
pub struct SmtLeafInput<'a> {
    pub lane_id: &'a LaneId,
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
