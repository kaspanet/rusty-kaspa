use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{ghostdag::GhostdagStoreReader, reachability::ReachabilityStoreReader, relations::RelationsStoreReader},
};
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::BlockHashExtensions,
    errors::traversal::{TraversalError, TraversalResult},
    BlockHashSet, ChainPath,
};
use kaspa_core::trace;
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

    pub fn calculate_chain_path(&self, from: Hash, to: Hash, chain_path_added_limit: Option<usize>) -> ChainPath {
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
        if chain_path_added_limit.is_none() {
            // Use backward chain iterator
            // It is more intuitive to use forward iterator here, but going downwards the selected chain is faster.
            let mut added = self.reachability_service.backward_chain_iterator(to, common_ancestor, false).collect_vec();
            added.reverse();
            return ChainPath { added, removed };
        }
        // Use forward chain iterator, to ascertain a path from the common ancestor to the target.
        let added = self
            .reachability_service
            .forward_chain_iterator(common_ancestor, to, true)
            .skip(1)
            .take(chain_path_added_limit.unwrap()) // we handle is_none so we may unwrap. 
            .collect_vec();
        ChainPath { added, removed }
    }

    pub fn anticone(
        &self,
        block: Hash,
        tips: impl Iterator<Item = Hash>,
        max_traversal_allowed: Option<u64>,
    ) -> TraversalResult<Vec<Hash>> {
        self.antipast_traversal(tips, block, max_traversal_allowed, true)
    }

    pub fn antipast(
        &self,
        block: Hash,
        tips: impl Iterator<Item = Hash>,
        max_traversal_allowed: Option<u64>,
    ) -> TraversalResult<Vec<Hash>> {
        self.antipast_traversal(tips, block, max_traversal_allowed, false)
    }

    fn antipast_traversal(
        &self,
        tips: impl Iterator<Item = Hash>,
        block: Hash,
        max_traversal_allowed: Option<u64>,
        return_anticone_only: bool,
    ) -> Result<Vec<Hash>, TraversalError> {
        //  In some cases we search for the anticone of the pruning point starting from virtual parents.
        //  This means we might traverse ~pruning_depth blocks which are all stored in the visited set.
        //  Experiments (and theory) show that w/o completely tracking visited, the queue might grow in
        //  size quadratically due to many duplicate blocks, easily resulting in OOM errors if the DAG is
        //  wide. On the other hand, even at 10 BPS, pruning depth is around 2M blocks which is approx 64MB, a modest
        //  memory peak which happens at most once a in a pruning period (since pruning anticone is cached).
        let mut output = Vec::new(); // Anticone or antipast, depending on args
        let mut queue = VecDeque::from_iter(tips);
        let mut visited = BlockHashSet::from_iter(queue.iter().copied());
        let mut traversal_count = 0;
        while let Some(current) = queue.pop_front() {
            // We reached a block in `past(block)` so we can terminate the BFS from this point on
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

            if traversal_count % 10000 == 0 {
                trace!(
                    "[TRAVERSAL MANAGER] Traversal count: {}, queue size: {}, anticone size: {}, visited size: {}",
                    traversal_count,
                    queue.len(),
                    output.len(),
                    visited.len()
                );
            }
            // At this point, we know `current` is in antipast of `block`. The second condition is there to check if it's in the anticone
            if !return_anticone_only || !self.reachability_service.is_dag_ancestor_of(block, current) {
                output.push(current);
            }

            for parent in self.relations_store.get_parents(current).unwrap().iter().copied() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }

        Ok(output)
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
    // Returns all blocks on route on the bfs path from this to descendant
    pub fn forward_bfs_paths_iterator(&self, this: Hash, descendant: Hash) -> BlocksBfsPathsIterator<'_, U, V> {
        BlocksBfsPathsIterator::new(this, Some(descendant), &self.reachability_service, &self.relations_store, BfsDirection::Forward)
    }

    // Returns all  known blocks on route on the bfs path from this onward
    pub fn default_forward_bfs_paths_iterator(&self, this: Hash) -> BlocksBfsPathsIterator<'_, U, V> {
        BlocksBfsPathsIterator::new(this, None, &self.reachability_service, &self.relations_store, BfsDirection::Forward)
    }

    // Returns all nodes on route on the backward bfs path from this to ancestor
    pub fn backward_bfs_paths_iterator(&self, this: Hash, ancestor: Hash) -> BlocksBfsPathsIterator<'_, U, V> {
        BlocksBfsPathsIterator::new(this, Some(ancestor), &self.reachability_service, &self.relations_store, BfsDirection::Backward)
    }
    // Returns all nodes on route on the backward bfs path from this to genesis
    pub fn default_backward_bfs_paths_iterator(&self, this: Hash) -> BlocksBfsPathsIterator<'_, U, V> {
        BlocksBfsPathsIterator::new(this, None, &self.reachability_service, &self.relations_store, BfsDirection::Backward)
    }
}
#[derive(PartialEq, Eq)]
pub enum BfsDirection {
    Forward,
    Backward,
}
pub struct BlocksBfsPathsIterator<'a, U: ReachabilityStoreReader, V: RelationsStoreReader> {
    queue: VecDeque<Vec<Hash>>,
    visited: HashSet<Hash>,
    edge: Option<Hash>,
    reachability_service: &'a MTReachabilityService<U>,
    relations_store: &'a V,
    bfs_direction: BfsDirection,
}

impl<'a, U: ReachabilityStoreReader, V: RelationsStoreReader> BlocksBfsPathsIterator<'a, U, V> {
    pub fn new(
        start: Hash,
        edge: Option<Hash>,
        reachability_service: &'a MTReachabilityService<U>,
        relations_store: &'a V,
        bfs_direction: BfsDirection,
    ) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back(vec![start]);
        let mut visited = HashSet::new();
        visited.insert(start); // Note that in a dag this isn't actually necessary, but is kept for logical clarity
        Self { queue, visited, edge, reachability_service, relations_store, bfs_direction }
    }
    pub fn map_paths_to_tips(self) -> impl Iterator<Item = Hash> + use<'a, U, V> {
        self.map(|path| path.last().cloned().unwrap())
    }
}
impl<U: ReachabilityStoreReader, V: RelationsStoreReader> Iterator for BlocksBfsPathsIterator<'_, U, V> {
    type Item = Vec<Hash>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut curr;
        let mut path;
        loop {
            // Loop until a block on the route is found
            if self.queue.is_empty() {
                return None;
            }
            path = self.queue.pop_front().unwrap();
            curr = *path.last().unwrap(); // Path should never be empty

            if self.edge.is_none()
                || self.bfs_direction == BfsDirection::Forward
                    && self.reachability_service.is_dag_ancestor_of_result(curr, self.edge.unwrap()).is_ok_and(|bool| bool)
                || self.bfs_direction == BfsDirection::Backward
                    && self.reachability_service.is_dag_ancestor_of_result(self.edge.unwrap(), curr).is_ok_and(|bool| bool)
            {
                // Once a block on the route is found in the queue, break out of loop
                break;
            }
        }
        let next_batch = match self.bfs_direction {
            BfsDirection::Forward => self.relations_store.get_children(curr).unwrap().read().iter().cloned().collect_vec(), // I feel like this can potentially panic
            BfsDirection::Backward => self.relations_store.get_parents(curr).unwrap_or(vec![].into()).iter().cloned().collect_vec(),
        };
        for &elem in next_batch.iter() {
            if !self.visited.contains(&elem) {
                let mut new_path = path.clone();
                new_path.push(elem);
                self.queue.push_back(new_path);
                self.visited.insert(elem);
            }
        }

        Some(path)
    }
}
