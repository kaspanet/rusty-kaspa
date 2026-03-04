//! Core types for KIP-0021 sequencing commitments.

use alloc::vec::Vec;
use kaspa_hashes::Hash;

/// Lane ID — 20-byte subnetwork identifier.
pub type LaneId = [u8; 20];

/// State of a single active lane in the sequencing commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveLaneEntry {
    /// Current recursive tip hash for this lane.
    pub lane_tip_hash: Hash,
    /// Blue score of the last block that touched this lane.
    pub last_touch_blue_score: u64,
}

/// Diff produced by applying a block to the active lanes state.
///
/// Records enough information to revert the block during reorgs.
#[derive(Clone, Debug, Default)]
pub struct ActiveLanesDiff {
    /// Lanes that were created or updated (with their new state).
    pub updated: Vec<(LaneId, ActiveLaneEntry)>,
    /// Lanes that were removed due to inactivity (with their prior state).
    pub removed: Vec<(LaneId, ActiveLaneEntry)>,
}

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
#[derive(Clone, Copy, Debug)]
pub struct SmtLeafInput<'a> {
    pub lane_key: &'a Hash,
    pub lane_tip: &'a Hash,
    pub blue_score: u64,
}

/// Components of the sequencing state root.
#[derive(Clone, Copy, Debug)]
pub struct SeqState<'a> {
    pub lanes_root: &'a Hash,
    pub context_hash: &'a Hash,
    pub payload_root: &'a Hash,
}

/// Input for the final sequencing commitment.
#[derive(Clone, Copy, Debug)]
pub struct SeqCommitInput<'a> {
    pub parent_seq_commit: &'a Hash,
    pub state_root: &'a Hash,
}
