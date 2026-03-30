use kaspa_hashes::Hash;

pub mod branch_version_store;
pub mod cache;
pub mod keys;
pub mod lane_version_store;
pub mod maybe_fork;
pub mod processor;
pub mod reverse_blue_score;
pub mod score_index;
pub mod values;

/// SMT key for a lane: `H_lane_key(lane_id)`. 256-bit position in the tree.
pub type LaneKey = Hash;

/// Block hash identifying a specific version.
pub type BlockHash = Hash;

/// Re-export the branch changes map type from kaspa-smt.
pub use kaspa_smt::tree::SmtBranchChanges;
