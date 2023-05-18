use std::{collections::VecDeque, sync::Arc};

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{ghostdag::GhostdagStoreReader, reachability::ReachabilityStoreReader, relations::RelationsStoreReader},
};
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::BlockHashExtensions,
    errors::traversal::{TraversalError, TraversalResult},
    BlockHashSet, ChainPath, HashMapCustomHasher,
};
use kaspa_hashes::Hash;

#[derive(Clone)]
pub struct DagTraversalManager<T: GhostdagStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> {
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    relations_store: V,
    reachability_service: MTReachabilityService<U>,
}

impl<T: GhostdagStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> DagTraversalManager<T, U, V> {
    pub fn new(
        genesis_hash: Hash,
        ghostdag_store: Arc<T>,
        relations_store: V,
        reachability_service: MTReachabilityService<U>,
    ) -> Self {
        Self { genesis_hash, ghostdag_store, relations_store, reachability_service }
    }

    pub fn calculate_chain_path(&self, from: Hash, to: Hash) -> ChainPath {
        let mut removed = Vec::new();
        let mut common_ancestor = from;
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if !self.reachability_service.is_chain_ancestor_of(current, to) {
                removed.push(current);
            } else {
                common_ancestor = current;
                break;
            }
        }

        let mut added = self.reachability_service.backward_chain_iterator(to, common_ancestor, false).collect_vec(); // It is more intuitive to use forward iterator here, but going downwards the selected chain is faster.
        added.reverse();
        ChainPath { added, removed }
    }

    pub fn anticone(
        &self,
        block: Hash,
        tips: impl Iterator<Item = Hash>,
        max_traversal_allowed: Option<u64>,
    ) -> TraversalResult<Vec<Hash>> {
        let mut anticone = Vec::new();
        let mut queue = VecDeque::from_iter(tips);
        let mut visited = BlockHashSet::new();
        let mut traversal_count = 0;
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }

            if self.reachability_service.is_dag_ancestor_of(current, block) {
                continue;
            }

            // We count the number of blocks in past(tips) \setminus past(block).
            // We don't use `visited.len()` since it includes some maximal blocks in past(block) as well.
            traversal_count += 1;
            if let Some(max_traversal_allowed) = max_traversal_allowed {
                if traversal_count > max_traversal_allowed {
                    return Err(TraversalError::ReachedMaxTraversalAllowed(traversal_count, max_traversal_allowed));
                }
            }

            if !self.reachability_service.is_dag_ancestor_of(block, current) {
                anticone.push(current);
            }

            for parent in self.relations_store.get_parents(current).unwrap().iter().copied() {
                queue.push_back(parent);
            }
        }

        Ok(anticone)
    }

    pub fn lowest_chain_block_above_or_equal_to_blue_score(&self, high: Hash, blue_score: u64) -> Hash {
        let high_gd = self.ghostdag_store.get_compact_data(high).unwrap();
        assert!(high_gd.blue_score >= blue_score);

        let mut current = high;
        let mut current_gd = high_gd;

        while current != self.genesis_hash {
            assert!(!current.is_origin(), "there's no such known block");
            let selected_parent_gd = self.ghostdag_store.get_compact_data(current_gd.selected_parent).unwrap();
            if selected_parent_gd.blue_score < blue_score {
                break;
            }

            current = current_gd.selected_parent;
            current_gd = selected_parent_gd;
        }

        current
    }
}
