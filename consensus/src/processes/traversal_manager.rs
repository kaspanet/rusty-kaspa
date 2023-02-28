use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    ops::Deref,
    sync::Arc,
};

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_window_cache::{BlockWindowCacheReader, BlockWindowHeap},
            ghostdag::{GhostdagData, GhostdagStoreReader},
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::ghostdag::ordering::SortableBlock,
};
use consensus_core::{
    blockhash::BlockHashExtensions,
    errors::{
        block::RuleError,
        traversal::{TraversalError, TraversalResult},
    },
    BlockHashSet, BlueWorkType, ChainPath, HashMapCustomHasher,
};
use hashes::Hash;
use itertools::Itertools;
use kaspa_utils::refs::Refs;

#[derive(Clone)]
pub struct DagTraversalManager<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: ReachabilityStoreReader, W: RelationsStoreReader>
{
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    relations_store: W,
    reachability_service: MTReachabilityService<V>,
    block_window_cache_for_difficulty: Arc<U>,
    block_window_cache_for_past_median_time: Arc<U>,
    difficulty_window_size: usize,
    past_median_time_window_size: usize,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: ReachabilityStoreReader, W: RelationsStoreReader>
    DagTraversalManager<T, U, V, W>
{
    pub fn new(
        genesis_hash: Hash,
        ghostdag_store: Arc<T>,
        relations_store: W,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        difficulty_window_size: usize,
        past_median_time_window_size: usize,
        reachability_service: MTReachabilityService<V>,
    ) -> Self {
        Self {
            genesis_hash,
            ghostdag_store,
            relations_store,
            block_window_cache_for_difficulty,
            difficulty_window_size,
            block_window_cache_for_past_median_time,
            past_median_time_window_size,
            reachability_service,
        }
    }

    pub fn block_window(&self, high_ghostdag_data: &GhostdagData, window_size: usize) -> Result<BlockWindowHeap, RuleError> {
        if window_size == 0 {
            return Ok(BlockWindowHeap::new());
        }

        let cache = if window_size == self.difficulty_window_size {
            Some(&self.block_window_cache_for_difficulty)
        } else if window_size == self.past_median_time_window_size {
            Some(&self.block_window_cache_for_past_median_time)
        } else {
            None
        };

        if let Some(cache) = cache {
            if let Some(selected_parent_binary_heap) = cache.get(&high_ghostdag_data.selected_parent) {
                let mut window_heap = BoundedSizeBlockHeap::from_binary_heap(window_size, (*selected_parent_binary_heap).clone());
                if high_ghostdag_data.selected_parent != self.genesis_hash {
                    self.try_push_mergeset(
                        &mut window_heap,
                        high_ghostdag_data,
                        self.ghostdag_store.get_blue_work(high_ghostdag_data.selected_parent).unwrap(),
                    );
                }

                return Ok(window_heap.binary_heap);
            }
        }

        let mut window_heap = BoundedSizeBlockHeap::new(window_size);
        let mut current_ghostdag: Refs<GhostdagData> = high_ghostdag_data.into();

        // Walk down the chain until we cross the window boundaries
        loop {
            if current_ghostdag.selected_parent.is_origin() {
                // Reaching origin means there's no more data, so we expect the window to already be full, otherwise we err.
                // This error can happen only during an IBD from pruning proof when processing the first headers in the pruning point's
                // future, and means that the syncer did not provide sufficient trusted information for proper validation
                if window_heap.reached_size_bound() {
                    break;
                } else {
                    return Err(RuleError::InsufficientDaaWindowSize(window_heap.binary_heap.len()));
                }
            }

            if current_ghostdag.selected_parent == self.genesis_hash {
                break;
            }

            let parent_ghostdag = self.ghostdag_store.get_data(current_ghostdag.selected_parent).unwrap();
            let selected_parent_blue_work_too_low =
                self.try_push_mergeset(&mut window_heap, &current_ghostdag, parent_ghostdag.blue_work);
            // No need to further iterate since past of selected parent has even lower blue work
            if selected_parent_blue_work_too_low {
                break;
            }
            current_ghostdag = parent_ghostdag.into();
        }

        Ok(window_heap.binary_heap)
    }

    fn try_push_mergeset(
        &self,
        heap: &mut BoundedSizeBlockHeap,
        ghostdag_data: &GhostdagData,
        selected_parent_blue_work: BlueWorkType,
    ) -> bool {
        // If the window is full and the selected parent is less than the minimum then we break
        // because this means that there cannot be any more blocks in the past with higher blue work
        if !heap.try_push(ghostdag_data.selected_parent, selected_parent_blue_work) {
            return true;
        }
        for block in ghostdag_data.descending_mergeset_without_selected_parent(self.ghostdag_store.deref()) {
            // If it's smaller than minimum then we won't be able to add the rest because we iterate in descending blue work order.
            if !heap.try_push(block.hash, block.blue_work) {
                break;
            }
        }
        false
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
}

struct BoundedSizeBlockHeap {
    binary_heap: BlockWindowHeap,
    size_bound: usize,
}

impl BoundedSizeBlockHeap {
    fn new(size_bound: usize) -> Self {
        Self::from_binary_heap(size_bound, BinaryHeap::with_capacity(size_bound))
    }

    fn from_binary_heap(size_bound: usize, binary_heap: BlockWindowHeap) -> Self {
        Self { size_bound, binary_heap }
    }

    fn reached_size_bound(&self) -> bool {
        self.binary_heap.len() == self.size_bound
    }

    fn try_push(&mut self, hash: Hash, blue_work: BlueWorkType) -> bool {
        let r_sortable_block = Reverse(SortableBlock { hash, blue_work });
        if self.reached_size_bound() {
            if let Some(max) = self.binary_heap.peek() {
                if *max < r_sortable_block {
                    return false; // Heap is full and the suggested block is greater than the max
                }
            }
            self.binary_heap.pop(); // Remove the max block (because it's reverse, it'll be the block with the least blue work)
        }
        self.binary_heap.push(r_sortable_block);
        true
    }
}
