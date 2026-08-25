use kaspa_hashes::Hash;

pub mod branch_version_store;
pub mod cache;
pub mod keys;
pub mod lane_version_store;
pub mod maybe_fork;
pub mod processor;
pub mod reacquire_iter;
pub mod reverse_blue_score;
pub mod score_index;
pub mod streaming_import;
pub mod values;

/// SMT key for a lane: `H_lane_key(lane_id)`. 256-bit position in the tree.
pub type LaneKey = Hash;

/// Block hash identifying a specific version.
pub type BlockHash = Hash;

/// Re-export node changes map type.
pub use kaspa_smt::tree::SmtNodeChanges;

/// Re-export Node enum.
pub use kaspa_smt::store::Node;
