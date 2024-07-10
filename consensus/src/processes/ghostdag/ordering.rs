use std::cmp::Ordering;

use kaspa_consensus_core::BlueWorkType;
use kaspa_core::warn;
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use serde::{Deserialize, Serialize};

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, relations::RelationsStoreReader},
};

use super::protocol::GhostdagManager;

#[derive(Eq, Clone, Serialize, Deserialize)]
pub struct SortableBlock {
    pub hash: Hash,
    pub blue_work: BlueWorkType,
}

impl SortableBlock {
    pub fn new(hash: Hash, blue_work: BlueWorkType) -> Self {
        Self { hash, blue_work }
    }
}

impl PartialEq for SortableBlock {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl PartialOrd for SortableBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.blue_work.cmp(&other.blue_work).then_with(|| self.hash.cmp(&other.hash))
    }
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn sort_blocks(&self, blocks: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = blocks.into_iter().collect();
        sorted_blocks.sort_by_cached_key(|block| SortableBlock {
            hash: *block,
            // TODO: Reconsider this approach
            // It's possible for mergeset.rs::unordered_mergeset_without_selected_parent (which calls this) to reference parents
            // that are in a lower level when calling relations.get_parents. This will panic at self.ghostdag_store.get_blue_work(*block)
            //
            // Options for fixes:
            // 1) do this where we simply unwrap and default to 0 (currently implemented)
            //    - consequence is that it affects all GD calculations
            //    - I argue this is fine for the short term because GD entries not being in the GD store
            //      can only happen IFF the parent is on a lower level. For level 0 (primary GD), this is not a problem
            //      and for higher GD it's also not a problem since we only want to use blocks in the same
            //      level or higher.
            //    - There is also an extra check being done in ghostdag call side to verify that the hashes in the mergeset
            //      belong to this
            // 2) in mergeset.rs::unordered_mergeset_without_selected_parent, guarantee that we're only getting
            //    parents that are in this store
            // 3) make relations store only return parents at the same or higher level
            //    - we know that realtions.get_parents can return parents in one level lower
            blue_work: self.ghostdag_store.get_blue_work(*block).unwrap_or_else(|_| {
                warn!("Tried getting blue work of hash not in GD store: {}", block);
                Uint192::from_u64(0)
            }),
        });
        sorted_blocks
    }
}
