use std::{cmp::Reverse, collections::BinaryHeap, ops::Deref, sync::Arc};

use crate::{
    model::stores::{
        block_window_cache::{BlockWindowCacheReader, BlockWindowHeap},
        ghostdag::{GhostdagData, GhostdagStoreReader},
    },
    processes::ghostdag::ordering::SortableBlock,
};
use consensus_core::{blockhash::BlockHashExtensions, BlueWorkType};
use hashes::Hash;

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
        genesis_hash: Hash, ghostdag_store: Arc<T>, block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>, difficulty_window_size: usize,
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
    pub fn block_window(&self, high_ghostdag_data: Arc<GhostdagData>, window_size: usize) -> BlockWindowHeap {
        let mut window_heap = SizedUpBlockHeap::new(self.ghostdag_store.clone(), window_size);
        if window_size == 0 {
            return window_heap.binary_heap;
        }

        let mut current_gd = high_ghostdag_data;

        let cache = if window_size == self.difficulty_window_size {
            Some(&self.block_window_cache_for_difficulty)
        } else if window_size == self.past_median_time_window_size {
            Some(&self.block_window_cache_for_past_median_time)
        } else {
            None
        };

        if let Some(cache) = cache {
            if let Some(selected_parent_binary_heap) = cache.get(&current_gd.selected_parent) {
                let mut window_heap = SizedUpBlockHeap::from_binary_heap(
                    self.ghostdag_store.clone(),
                    window_size,
                    (*selected_parent_binary_heap).clone(),
                );
                if current_gd.selected_parent != self.genesis_hash {
                    self.try_push_mergeset(&mut window_heap, &current_gd);
                }

                return window_heap.binary_heap;
            }
        }

        // Walk down the chain until we finish
        loop {
            assert!(!current_gd.selected_parent.is_origin(), "Block window should never get to the origin block");
            if current_gd.selected_parent == self.genesis_hash {
                break;
            }

            let done = self.try_push_mergeset(&mut window_heap, &current_gd);
            if done {
                break;
            }

            current_gd = self
                .ghostdag_store
                .get_data(current_gd.selected_parent)
                .unwrap();
        }

        window_heap.binary_heap
    }

    fn try_push_mergeset(&self, heap: &mut SizedUpBlockHeap<T>, ghostdag_data: &GhostdagData) -> bool {
        // If the window is full and the selected parent is less than the minimum then we break
        // because this means that there cannot be any more blocks in the past with higher blue work
        if !heap.try_push(ghostdag_data.selected_parent) {
            return true;
        }

        for block in ghostdag_data.descending_mergeset_without_selected_parent(self.ghostdag_store.deref()) {
            // If it's smaller than minimum then we won't be able to add the rest because we iterate in descending blue work order.
            if !heap.try_push_with_blue_work(block.hash, block.blue_work) {
                break;
            }
        }

        false
    }
}

struct SizedUpBlockHeap<T: GhostdagStoreReader> {
    binary_heap: BlockWindowHeap,
    ghostdag_store: Arc<T>,
    size: usize,
}

impl<T: GhostdagStoreReader> SizedUpBlockHeap<T> {
    fn new(ghostdag_store: Arc<T>, size: usize) -> Self {
        Self::from_binary_heap(ghostdag_store, size, BinaryHeap::new())
    }

    fn from_binary_heap(ghostdag_store: Arc<T>, size: usize, binary_heap: BlockWindowHeap) -> Self {
        Self { ghostdag_store, size, binary_heap }
    }

    fn try_push(&mut self, hash: Hash) -> bool {
        let blue_work = self.ghostdag_store.get_blue_work(hash).unwrap();
        self.try_push_with_blue_work(hash, blue_work)
    }

    fn try_push_with_blue_work(&mut self, hash: Hash, blue_work: BlueWorkType) -> bool {
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
