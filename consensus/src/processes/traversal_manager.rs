use std::{cmp::Reverse, collections::BinaryHeap, ops::Deref, sync::Arc};

use crate::{
    model::stores::{
        block_window_cache::{BlockWindowCacheReader, BlockWindowHeap},
        ghostdag::{GhostdagData, GhostdagStoreReader},
    },
    processes::ghostdag::ordering::SortableBlock,
};
use consensus_core::{blockhash::BlockHashExtensions, errors::block::RuleError, BlueWorkType};
use database::prelude::StoreResultExtensions;
use hashes::Hash;
use kaspa_utils::refs::Refs;

#[derive(Clone)]
pub struct DagTraversalManager<T: GhostdagStoreReader, U: BlockWindowCacheReader> {
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    block_window_cache_for_difficulty: Arc<U>,
    block_window_cache_for_past_median_time: Arc<U>,
    difficulty_window_size: usize,
    past_median_time_window_size: usize,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader> DagTraversalManager<T, U> {
    pub fn new(
        genesis_hash: Hash,
        ghostdag_store: Arc<T>,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        difficulty_window_size: usize,
        past_median_time_window_size: usize,
    ) -> Self {
        Self {
            genesis_hash,
            ghostdag_store,
            block_window_cache_for_difficulty,
            difficulty_window_size,
            block_window_cache_for_past_median_time,
            past_median_time_window_size,
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
                if window_heap.binary_heap.len() == window_heap.size {
                    break;
                } else {
                    return Err(RuleError::InsufficientDaaWindowSize(window_heap.binary_heap.len()));
                }
            }

            if current_ghostdag.selected_parent == self.genesis_hash {
                break;
            }

            let Some(parent_ghostdag) = self.ghostdag_store.get_data(current_ghostdag.selected_parent).unwrap_option() else {
                if window_heap.binary_heap.len() == window_heap.size {
                    break;
                } else {
                    return Err(RuleError::InsufficientDaaWindowSize(window_heap.binary_heap.len()));
                }
            };

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
}

struct BoundedSizeBlockHeap {
    binary_heap: BlockWindowHeap,
    size: usize,
}

impl BoundedSizeBlockHeap {
    fn new(size: usize) -> Self {
        Self::from_binary_heap(size, BinaryHeap::with_capacity(size))
    }

    fn from_binary_heap(size: usize, binary_heap: BlockWindowHeap) -> Self {
        Self { size, binary_heap }
    }

    fn try_push(&mut self, hash: Hash, blue_work: BlueWorkType) -> bool {
        let r_sortable_block = Reverse(SortableBlock { hash, blue_work });
        if self.binary_heap.len() == self.size {
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
