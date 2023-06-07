use crate::{
    model::stores::{
        block_window_cache::{BlockWindowCacheReader, BlockWindowHeap, WindowOrigin},
        daa::DaaStoreReader,
        ghostdag::{GhostdagData, GhostdagStoreReader},
        headers::HeaderStoreReader,
    },
    processes::ghostdag::ordering::SortableBlock,
};
use kaspa_consensus_core::{
    blockhash::BlockHashExtensions,
    config::genesis::GenesisBlock,
    errors::{block::RuleError, difficulty::DifficultyResult},
    BlockHashSet, BlueWorkType,
};
use kaspa_hashes::Hash;
use kaspa_math::Uint256;
use kaspa_utils::refs::Refs;
use once_cell::unsync::Lazy;
use std::{cmp::Reverse, iter::once, ops::Deref, sync::Arc};

use super::{
    difficulty::{FullDifficultyManager, SampledDifficultyManager},
    past_median_time::{FullPastMedianTimeManager, SampledPastMedianTimeManager},
};

#[derive(Clone, Copy)]
pub enum WindowType {
    SampledDifficultyWindow,
    FullDifficultyWindow,
    SampledMedianTimeWindow,
    VaryingWindow(usize),
}

pub struct DaaWindow {
    pub window: Arc<BlockWindowHeap>,
    pub daa_score: u64,
    pub mergeset_non_daa: BlockHashSet,
}

impl DaaWindow {
    pub fn new(window: Arc<BlockWindowHeap>, daa_score: u64, mergeset_non_daa: BlockHashSet) -> Self {
        Self { window, daa_score, mergeset_non_daa }
    }
}

pub trait WindowManager {
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError>;
    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow;
    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError>;
    fn calculate_difficulty_bits(&self, ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32;
    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError>;
    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64>;
    fn window_size(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> usize;
    fn sample_rate(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> u64;
}

/// A window manager conforming (indirectly) to the legacy golang implementation
/// based on full, hence un-sampled, windows
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
        genesis: &GenesisBlock,
        ghostdag_store: Arc<T>,
        headers_store: Arc<V>,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        target_time_per_block: u64,
        difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        past_median_time_window_size: usize,
    ) -> Self {
        let difficulty_manager = FullDifficultyManager::new(
            headers_store.clone(),
            genesis.bits,
            difficulty_window_size,
            min_difficulty_window_len,
            target_time_per_block,
        );
        let past_median_time_manager = FullPastMedianTimeManager::new(headers_store, genesis.timestamp);
        Self {
            genesis_hash: genesis.hash,
            ghostdag_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            difficulty_window_size,
            past_median_time_window_size,
            difficulty_manager,
            past_median_time_manager,
        }
    }

    fn build_block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        let window_size = self.window_size(ghostdag_data, window_type);
        if window_size == 0 {
            return Ok(Arc::new(BlockWindowHeap::new(WindowOrigin::Full)));
        }

        let cache = if window_size == self.difficulty_window_size {
            Some(&self.block_window_cache_for_difficulty)
        } else if window_size == self.past_median_time_window_size {
            Some(&self.block_window_cache_for_past_median_time)
        } else {
            None
        };

        if let Some(cache) = cache {
            if let Some(selected_parent_binary_heap) = cache.get(&ghostdag_data.selected_parent) {
                // Only use the cached window if it originates from here
                if let WindowOrigin::Full = selected_parent_binary_heap.origin() {
                    let mut window_heap = BoundedSizeBlockHeap::from_binary_heap(window_size, (*selected_parent_binary_heap).clone());
                    if ghostdag_data.selected_parent != self.genesis_hash {
                        self.try_push_mergeset(
                            &mut window_heap,
                            ghostdag_data,
                            self.ghostdag_store.get_blue_work(ghostdag_data.selected_parent).unwrap(),
                        );
                    }

                    return Ok(Arc::new(window_heap.binary_heap));
                }
            }
        }

        let mut window_heap = BoundedSizeBlockHeap::new(WindowOrigin::Full, window_size);
        let mut current_ghostdag: Refs<GhostdagData> = ghostdag_data.into();

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

        Ok(Arc::new(window_heap.binary_heap))
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

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader> WindowManager for FullWindowManager<T, U, V> {
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        self.build_block_window(ghostdag_data, window_type)
    }

    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow {
        let (daa_score, mergeset_non_daa) =
            self.difficulty_manager.calc_daa_score_and_mergeset_non_daa_blocks(&window, ghostdag_data, self.ghostdag_store.deref());
        DaaWindow::new(window, daa_score, mergeset_non_daa)
    }

    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::SampledDifficultyWindow)?;
        Ok(self.calc_daa_window(ghostdag_data, window))
    }

    fn calculate_difficulty_bits(&self, _high_ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        self.difficulty_manager.calculate_difficulty_bits(&daa_window.window)
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::SampledMedianTimeWindow)?;
        let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
        Ok((past_median_time, window))
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.difficulty_manager.estimate_network_hashes_per_second(&window)
    }

    fn window_size(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match window_type {
            WindowType::SampledDifficultyWindow | WindowType::FullDifficultyWindow => self.difficulty_window_size,
            WindowType::SampledMedianTimeWindow => self.past_median_time_window_size,
            WindowType::VaryingWindow(size) => size,
        }
    }

    fn sample_rate(&self, _ghostdag_data: &GhostdagData, _window_type: WindowType) -> u64 {
        1
    }
}

type DaaStatus = Option<(u64, BlockHashSet)>;

enum SampledBlock {
    Sampled(SortableBlock),
    NonDaa(Hash),
}

/// A sampled window manager implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md)
#[derive(Clone)]
pub struct SampledWindowManager<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> {
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    headers_store: Arc<V>,
    daa_store: Arc<W>,
    block_window_cache_for_difficulty: Arc<U>,
    block_window_cache_for_past_median_time: Arc<U>,
    target_time_per_block: u64,
    sampling_activation_daa_score: u64,
    difficulty_window_size: usize,
    difficulty_sample_rate: u64,
    past_median_time_window_size: usize,
    past_median_time_sample_rate: u64,
    difficulty_manager: SampledDifficultyManager<V>,
    past_median_time_manager: SampledPastMedianTimeManager<V>,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> SampledWindowManager<T, U, V, W> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis: &GenesisBlock,
        ghostdag_store: Arc<T>,
        headers_store: Arc<V>,
        daa_store: Arc<W>,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        max_difficulty_target: Uint256,
        target_time_per_block: u64,
        sampling_activation_daa_score: u64,
        difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        difficulty_sample_rate: u64,
        past_median_time_window_size: usize,
        past_median_time_sample_rate: u64,
    ) -> Self {
        let difficulty_manager = SampledDifficultyManager::new(
            headers_store.clone(),
            genesis.bits,
            max_difficulty_target,
            difficulty_window_size,
            min_difficulty_window_len,
            difficulty_sample_rate,
            target_time_per_block,
        );
        let past_median_time_manager = SampledPastMedianTimeManager::new(headers_store.clone(), genesis.timestamp);
        Self {
            genesis_hash: genesis.hash,
            ghostdag_store,
            headers_store,
            daa_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            target_time_per_block,
            sampling_activation_daa_score,
            difficulty_window_size,
            difficulty_sample_rate,
            past_median_time_window_size,
            past_median_time_sample_rate,
            difficulty_manager,
            past_median_time_manager,
        }
    }

    fn build_block_window(
        &self,
        ghostdag_data: &GhostdagData,
        window_type: WindowType,
        mut mergeset_non_daa_inserter: impl FnMut(Hash),
    ) -> Result<Arc<BlockWindowHeap>, RuleError> {
        let window_size = self.window_size(ghostdag_data, window_type);
        let sample_rate = self.sample_rate(ghostdag_data, window_type);

        // First, we handle all edge cases
        if window_size == 0 {
            return Ok(Arc::new(BlockWindowHeap::new(WindowOrigin::Sampled)));
        }
        if ghostdag_data.selected_parent == self.genesis_hash {
            // Special case: Genesis does not enter the DAA window due to having a fixed timestamp
            mergeset_non_daa_inserter(self.genesis_hash);
            return Ok(Arc::new(BlockWindowHeap::new(WindowOrigin::Sampled)));
        }
        if ghostdag_data.selected_parent.is_origin() {
            return Err(RuleError::InsufficientDaaWindowSize(0));
        }

        let cache = match window_type {
            WindowType::SampledDifficultyWindow => Some(&self.block_window_cache_for_difficulty),
            WindowType::SampledMedianTimeWindow => Some(&self.block_window_cache_for_past_median_time),
            WindowType::FullDifficultyWindow | WindowType::VaryingWindow(_) => None,
        };

        if let Some(cache) = cache {
            if let Some(selected_parent_binary_heap) = cache.get(&ghostdag_data.selected_parent) {
                // Only use the cached window if it originates from here
                if let WindowOrigin::Sampled = selected_parent_binary_heap.origin() {
                    let selected_parent_blue_work = self.ghostdag_store.get_blue_work(ghostdag_data.selected_parent).unwrap();

                    let mut heap =
                        Lazy::new(|| BoundedSizeBlockHeap::from_binary_heap(window_size, (*selected_parent_binary_heap).clone()));
                    for block in self.sampled_mergeset_iterator(sample_rate, ghostdag_data, selected_parent_blue_work) {
                        match block {
                            SampledBlock::Sampled(block) => {
                                heap.try_push(block.hash, block.blue_work);
                            }
                            SampledBlock::NonDaa(hash) => {
                                mergeset_non_daa_inserter(hash);
                            }
                        }
                    }

                    return if let Ok(heap) = Lazy::into_value(heap) {
                        Ok(Arc::new(heap.binary_heap))
                    } else {
                        Ok(selected_parent_binary_heap.clone())
                    };
                }
            }
        }

        let mut window_heap = BoundedSizeBlockHeap::new(WindowOrigin::Sampled, window_size);
        let parent_ghostdag = self.ghostdag_store.get_data(ghostdag_data.selected_parent).unwrap();

        for block in self.sampled_mergeset_iterator(sample_rate, ghostdag_data, parent_ghostdag.blue_work) {
            match block {
                SampledBlock::Sampled(block) => {
                    window_heap.try_push(block.hash, block.blue_work);
                }
                SampledBlock::NonDaa(hash) => {
                    mergeset_non_daa_inserter(hash);
                }
            }
        }

        let mut current_ghostdag = parent_ghostdag;

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
                self.try_push_mergeset(&mut window_heap, sample_rate, &current_ghostdag, parent_ghostdag.blue_work);
            // No need to further iterate since past of selected parent has even lower blue work
            if selected_parent_blue_work_too_low {
                break;
            }

            current_ghostdag = parent_ghostdag;
        }

        Ok(Arc::new(window_heap.binary_heap))
    }

    fn try_push_mergeset(
        &self,
        heap: &mut BoundedSizeBlockHeap,
        sample_rate: u64,
        ghostdag_data: &GhostdagData,
        selected_parent_blue_work: BlueWorkType,
    ) -> bool {
        // If the window is full and the selected parent is less than the minimum then we break
        // because this means that there cannot be any more blocks in the past with higher blue work
        if !heap.can_push(ghostdag_data.selected_parent, selected_parent_blue_work) {
            return true;
        }

        for block in self.sampled_mergeset_iterator(sample_rate, ghostdag_data, selected_parent_blue_work) {
            match block {
                SampledBlock::Sampled(block) => {
                    if !heap.try_push(block.hash, block.blue_work) {
                        break;
                    }
                }
                SampledBlock::NonDaa(_) => {}
            }
        }
        false
    }

    fn sampled_mergeset_iterator<'a>(
        &'a self,
        sample_rate: u64,
        ghostdag_data: &'a GhostdagData,
        selected_parent_blue_work: BlueWorkType,
    ) -> impl Iterator<Item = SampledBlock> + 'a {
        let selected_parent_block = SortableBlock::new(ghostdag_data.selected_parent, selected_parent_blue_work);
        let selected_parent_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        let blue_score_threshold = self.difficulty_manager.lowest_daa_blue_score(ghostdag_data);
        let mut index: u64 = 0;

        once(selected_parent_block)
            .chain(ghostdag_data.descending_mergeset_without_selected_parent(self.ghostdag_store.deref()))
            .filter_map(move |block| {
                if self.ghostdag_store.get_blue_score(block.hash).unwrap() < blue_score_threshold {
                    Some(SampledBlock::NonDaa(block.hash))
                } else {
                    index += 1;
                    if (selected_parent_daa_score + index) % sample_rate == 0 {
                        Some(SampledBlock::Sampled(block))
                    } else {
                        None
                    }
                }
            })
    }
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> WindowManager
    for SampledWindowManager<T, U, V, W>
{
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        self.build_block_window(ghostdag_data, window_type, |_| {})
    }

    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow {
        let (daa_score, mergeset_non_daa) =
            self.difficulty_manager.calc_daa_score_and_mergeset_non_daa_blocks(ghostdag_data, self.ghostdag_store.deref());
        DaaWindow::new(window, daa_score, mergeset_non_daa)
    }

    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError> {
        let mut mergeset_non_daa = BlockHashSet::default();
        let window = self.build_block_window(ghostdag_data, WindowType::SampledDifficultyWindow, |hash| {
            mergeset_non_daa.insert(hash);
        })?;
        let daa_score = self.difficulty_manager.calc_daa_score(ghostdag_data, &mergeset_non_daa);
        Ok(DaaWindow::new(window, daa_score, mergeset_non_daa))
    }

    fn calculate_difficulty_bits(&self, _high_ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        self.difficulty_manager.calculate_difficulty_bits(&daa_window.window)
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::SampledMedianTimeWindow)?;
        let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
        Ok((past_median_time, window))
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.difficulty_manager.estimate_network_hashes_per_second(&window)
    }

    fn window_size(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match window_type {
            WindowType::SampledDifficultyWindow => self.difficulty_window_size,
            // We aim to return a full window such that it contains what would be the sampled window. Note that the
            // product below addresses also the worst-case scenario where the last sampled block is exactly `sample_rate`
            // blocks from the end of the full window
            WindowType::FullDifficultyWindow => self.difficulty_window_size * self.difficulty_sample_rate as usize,
            WindowType::SampledMedianTimeWindow => self.past_median_time_window_size,
            WindowType::VaryingWindow(size) => size,
        }
    }

    fn sample_rate(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> u64 {
        match window_type {
            WindowType::SampledDifficultyWindow => self.difficulty_sample_rate,
            WindowType::SampledMedianTimeWindow => self.past_median_time_sample_rate,
            WindowType::FullDifficultyWindow | WindowType::VaryingWindow(_) => 1,
        }
    }
}

/// A window manager handling either full (un-sampled) or sampled windows depending on an activation DAA score
///
/// See [FullWindowManager] and [SampledWindowManager]
#[derive(Clone)]
pub struct DualWindowManager<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> {
    ghostdag_store: Arc<T>,
    headers_store: Arc<V>,
    sampling_activation_daa_score: u64,
    full_window_manager: FullWindowManager<T, U, V>,
    sampled_window_manager: SampledWindowManager<T, U, V, W>,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> DualWindowManager<T, U, V, W> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis: &GenesisBlock,
        ghostdag_store: Arc<T>,
        headers_store: Arc<V>,
        daa_store: Arc<W>,
        block_window_cache_for_difficulty: Arc<U>,
        block_window_cache_for_past_median_time: Arc<U>,
        max_difficulty_target: Uint256,
        target_time_per_block: u64,
        sampling_activation_daa_score: u64,
        full_difficulty_window_size: usize,
        sampled_difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        difficulty_sample_rate: u64,
        full_past_median_time_window_size: usize,
        sampled_past_median_time_window_size: usize,
        past_median_time_sample_rate: u64,
    ) -> Self {
        let full_window_manager = FullWindowManager::new(
            genesis,
            ghostdag_store.clone(),
            headers_store.clone(),
            block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time.clone(),
            target_time_per_block,
            full_difficulty_window_size,
            min_difficulty_window_len.min(full_difficulty_window_size),
            full_past_median_time_window_size,
        );
        let sampled_window_manager = SampledWindowManager::new(
            genesis,
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            max_difficulty_target,
            target_time_per_block,
            sampling_activation_daa_score,
            sampled_difficulty_window_size,
            min_difficulty_window_len.min(sampled_difficulty_window_size),
            difficulty_sample_rate,
            sampled_past_median_time_window_size,
            past_median_time_sample_rate,
        );
        Self { ghostdag_store, headers_store, sampled_window_manager, full_window_manager, sampling_activation_daa_score }
    }

    fn sampling(&self, ghostdag_data: &GhostdagData) -> bool {
        let sp_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        sp_daa_score >= self.sampling_activation_daa_score
    }
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader, V: HeaderStoreReader, W: DaaStoreReader> WindowManager
    for DualWindowManager<T, U, V, W>
{
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.block_window(ghostdag_data, window_type),
            false => self.full_window_manager.block_window(ghostdag_data, window_type),
        }
    }

    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.calc_daa_window(ghostdag_data, window),
            false => self.full_window_manager.calc_daa_window(ghostdag_data, window),
        }
    }

    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError> {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.block_daa_window(ghostdag_data),
            false => self.full_window_manager.block_daa_window(ghostdag_data),
        }
    }

    fn calculate_difficulty_bits(&self, ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.calculate_difficulty_bits(ghostdag_data, daa_window),
            false => self.full_window_manager.calculate_difficulty_bits(ghostdag_data, daa_window),
        }
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.calc_past_median_time(ghostdag_data),
            false => self.full_window_manager.calc_past_median_time(ghostdag_data),
        }
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.sampled_window_manager.estimate_network_hashes_per_second(window)
    }

    fn window_size(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.window_size(ghostdag_data, window_type),
            false => self.full_window_manager.window_size(ghostdag_data, window_type),
        }
    }

    fn sample_rate(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> u64 {
        match self.sampling(ghostdag_data) {
            true => self.sampled_window_manager.sample_rate(ghostdag_data, window_type),
            false => self.full_window_manager.sample_rate(ghostdag_data, window_type),
        }
    }
}

struct BoundedSizeBlockHeap {
    binary_heap: BlockWindowHeap,
    size_bound: usize,
}

impl BoundedSizeBlockHeap {
    fn new(contents: WindowOrigin, size_bound: usize) -> Self {
        Self::from_binary_heap(size_bound, BlockWindowHeap::with_capacity(contents, size_bound))
    }

    fn from_binary_heap(size_bound: usize, binary_heap: BlockWindowHeap) -> Self {
        Self { size_bound, binary_heap }
    }

    fn reached_size_bound(&self) -> bool {
        self.binary_heap.len() == self.size_bound
    }

    fn can_push(&self, hash: Hash, blue_work: BlueWorkType) -> bool {
        let r_sortable_block = Reverse(SortableBlock { hash, blue_work });
        if self.reached_size_bound() {
            let max = self.binary_heap.peek().unwrap();
            // Returns false if heap is full and the suggested block is greater than the max. Since the heap is reversed,
            // pushing the suggested block would involve removing a block with a higher blue work.
            return *max >= r_sortable_block;
        }
        true
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
