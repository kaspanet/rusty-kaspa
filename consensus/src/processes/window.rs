use crate::{
    model::stores::{
        block_window_cache::{BlockWindowCacheReader, BlockWindowHeap},
        ghostdag::{GhostdagData, GhostdagStoreReader},
        headers::HeaderStoreReader,
    },
    processes::ghostdag::ordering::SortableBlock,
};
use kaspa_consensus_core::{
    blockhash::BlockHashExtensions,
    errors::{block::RuleError, difficulty::DifficultyResult},
    BlockHashSet, BlueWorkType,
};
use kaspa_hashes::Hash;
use kaspa_utils::refs::Refs;
use std::{cmp::Reverse, collections::BinaryHeap, ops::Deref, sync::Arc};

use super::{difficulty::FullDifficultyManager, past_median_time::FullPastMedianTimeManager};

pub enum WindowType {
    SampledDifficultyWindow,
    FullDifficultyWindow,
    SampledMedianTimeWindow,
    VaryingWindow(usize),
}

pub trait WindowManager {
    fn block_window(&self, high_ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<BlockWindowHeap, RuleError>;
    fn block_window_with_daa_score_and_non_daa_mergeset(
        &self,
        ghostdag_data: &GhostdagData,
    ) -> Result<(BlockWindowHeap, u64, BlockHashSet), RuleError>;
    fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32;
    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, BlockWindowHeap), RuleError>;
    fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64>;
}

#[derive(Clone)]
pub struct FullWindowManager<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader> {
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    block_window_cache_for_difficulty: Arc<U>,
    block_window_cache_for_past_median_time: Arc<U>,
    difficulty_window_size: usize,
    past_median_time_window_size: usize,
    difficulty_manager: FullDifficultyManager<V>,
    past_median_time_manager: FullPastMedianTimeManager<V>,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader> FullWindowManager<T, U, V> {
    pub fn new(
        genesis_hash: Hash,
        ghostdag_store: Arc<T>,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        difficulty_window_size: usize,
        past_median_time_window_size: usize,
        difficulty_manager: FullDifficultyManager<V>,
        past_median_time_manager: FullPastMedianTimeManager<V>,
    ) -> Self {
        Self {
            genesis_hash,
            ghostdag_store,
            block_window_cache_for_difficulty,
            difficulty_window_size,
            block_window_cache_for_past_median_time,
            past_median_time_window_size,
            difficulty_manager,
            past_median_time_manager,
        }
    }

    fn build_block_window(&self, high_ghostdag_data: &GhostdagData, window_size: usize) -> Result<BlockWindowHeap, RuleError> {
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

    fn window_size(&self, window_type: WindowType) -> usize {
        match window_type {
            WindowType::SampledDifficultyWindow | WindowType::FullDifficultyWindow => self.difficulty_window_size,
            WindowType::SampledMedianTimeWindow => self.past_median_time_window_size,
            WindowType::VaryingWindow(size) => size,
        }
    }
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader> WindowManager for FullWindowManager<T, U, V> {
    fn block_window(&self, high_ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<BlockWindowHeap, RuleError> {
        self.build_block_window(high_ghostdag_data, self.window_size(window_type))
    }

    fn block_window_with_daa_score_and_non_daa_mergeset(
        &self,
        ghostdag_data: &GhostdagData,
    ) -> Result<(BlockWindowHeap, u64, BlockHashSet), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::SampledDifficultyWindow)?;
        let (daa_score, non_daa_mergeset) =
            self.difficulty_manager.calc_daa_score_and_non_daa_mergeset_blocks(&window, ghostdag_data, self.ghostdag_store.deref());
        Ok((window, daa_score, non_daa_mergeset))
    }

    fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32 {
        self.difficulty_manager.calculate_difficulty_bits(window)
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, BlockWindowHeap), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::SampledMedianTimeWindow)?;
        let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
        Ok((past_median_time, window))
    }

    fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.difficulty_manager.estimate_network_hashes_per_second(window)
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
