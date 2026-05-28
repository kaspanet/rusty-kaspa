use std::sync::Arc;

use kaspa_consensus_core::KType;
use kaspa_hashes::Hash;

use crate::processes::ghostdag::ordering::SortableBlock;

pub mod manager;
pub mod protocol;
pub mod rank_search;
pub mod tie_breaking;

pub struct GroupMetadata {
    conflict_genesis: Hash,
    subgroup: Arc<Vec<Hash>>,
    k: KType,
    selected_parent: SortableBlock,
}
