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
    config::{genesis::GenesisBlock, params::ForkActivation},
    errors::{block::RuleError, difficulty::DifficultyResult},
    BlockHashSet, BlueWorkType,
};
use kaspa_hashes::Hash;
use kaspa_math::Uint256;
use kaspa_utils::refs::Refs;
use once_cell::unsync::Lazy;
use std::{
    cmp::Reverse,
    iter::once,
    ops::{Deref, DerefMut},
    sync::Arc,
};

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
        max_difficulty_target: Uint256,
        target_time_per_block: u64,
        difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        past_median_time_window_size: usize,
    ) -> Self {
        let difficulty_manager = FullDifficultyManager::new(
            headers_store.clone(),
            genesis.bits,
            max_difficulty_target,
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
    sampling_activation: ForkActivation,
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
        sampling_activation: ForkActivation,
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
            sampling_activation,
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

        let selected_parent_blue_work = self.ghostdag_store.get_blue_work(ghostdag_data.selected_parent).unwrap();

        // Try to initialize the window from the cache directly
        if let Some(res) = self.try_init_from_cache(
            window_size,
            sample_rate,
            cache,
            ghostdag_data,
            selected_parent_blue_work,
            Some(&mut mergeset_non_daa_inserter),
        ) {
            return Ok(res);
        }

        // else we populate the window with the passed ghostdag_data.
        let mut window_heap = BoundedSizeBlockHeap::new(WindowOrigin::Sampled, window_size);
        self.push_mergeset(
            &mut &mut window_heap,
            sample_rate,
            ghostdag_data,
            selected_parent_blue_work,
            Some(&mut mergeset_non_daa_inserter),
        );
        let mut current_ghostdag = self.ghostdag_store.get_data(ghostdag_data.selected_parent).unwrap();

        // Note: no need to check for cache here, as we already tried to initialize from the passed ghostdag's selected parent cache in `self.try_init_from_cache`

        // Walk down the chain until we cross the window boundaries.
        loop {
            // check if we may exit early.
            if current_ghostdag.selected_parent.is_origin() {
                // Reaching origin means there's no more data, so we expect the window to already be full, otherwise we err.
                // This error can happen only during an IBD from pruning proof when processing the first headers in the pruning point's
                // future, and means that the syncer did not provide sufficient trusted information for proper validation
                if window_heap.reached_size_bound() {
                    break;
                } else {
                    return Err(RuleError::InsufficientDaaWindowSize(window_heap.binary_heap.len()));
                }
            } else if current_ghostdag.selected_parent == self.genesis_hash {
                break;
            }

            let parent_ghostdag = self.ghostdag_store.get_data(current_ghostdag.selected_parent).unwrap();

            // No need to further iterate since past of selected parent has only lower blue work
            if !window_heap.can_push(current_ghostdag.selected_parent, parent_ghostdag.blue_work) {
                break;
            }

            // push the current mergeset into the window
            self.push_mergeset(&mut &mut window_heap, sample_rate, &current_ghostdag, parent_ghostdag.blue_work, None::<fn(Hash)>);

            // see if we can inherit and merge with the selected parent cache
            if self.try_merge_with_selected_parent_cache(&mut window_heap, cache, &current_ghostdag.selected_parent) {
                // if successful, we may break out of the loop, with the window already filled.
                break;
            };

            // update the current ghostdag to the parent ghostdag, and continue the loop.
            current_ghostdag = parent_ghostdag;
        }

        Ok(Arc::new(window_heap.binary_heap))
    }

    /// Push the mergeset samples into the bounded heap.
    /// Note: receives the heap argument as a DerefMut so that Lazy can be passed and be evaluated *only if an actual push is needed*
    fn push_mergeset(
        &self,
        heap: &mut impl DerefMut<Target = BoundedSizeBlockHeap>,
        sample_rate: u64,
        ghostdag_data: &GhostdagData,
        selected_parent_blue_work: BlueWorkType,
        mergeset_non_daa_inserter: Option<impl FnMut(Hash)>,
    ) {
        if let Some(mut mergeset_non_daa_inserter) = mergeset_non_daa_inserter {
            // If we have a non-daa inserter, we most iterate over the whole mergeset and op the sampled and non-daa blocks.
            for block in self.sampled_mergeset_iterator(sample_rate, ghostdag_data, selected_parent_blue_work) {
                match block {
                    SampledBlock::Sampled(block) => {
                        heap.try_push(block.hash, block.blue_work);
                    }
                    SampledBlock::NonDaa(hash) => mergeset_non_daa_inserter(hash),
                };
            }
        } else {
            // If we don't have a non-daa inserter, we can iterate over the sampled mergeset and return early if we can't push anymore.
            for block in self.sampled_mergeset_iterator(sample_rate, ghostdag_data, selected_parent_blue_work) {
                if let SampledBlock::Sampled(block) = block {
                    if !heap.try_push(block.hash, block.blue_work) {
                        return;
                    }
                }
            }
        }
    }

    fn try_init_from_cache(
        &self,
        window_size: usize,
        sample_rate: u64,
        cache: Option<&Arc<U>>,
        ghostdag_data: &GhostdagData,
        selected_parent_blue_work: BlueWorkType,
        mergeset_non_daa_inserter: Option<impl FnMut(Hash)>,
    ) -> Option<Arc<BlockWindowHeap>> {
        cache.and_then(|cache| {
            cache.get(&ghostdag_data.selected_parent).map(|selected_parent_window| {
                let mut heap = Lazy::new(|| BoundedSizeBlockHeap::from_binary_heap(window_size, (*selected_parent_window).clone()));
                // We pass a Lazy heap as an optimization to avoid cloning the selected parent heap in cases where the mergeset contains no samples
                self.push_mergeset(&mut heap, sample_rate, ghostdag_data, selected_parent_blue_work, mergeset_non_daa_inserter);
                if let Ok(heap) = Lazy::into_value(heap) {
                    Arc::new(heap.binary_heap)
                } else {
                    selected_parent_window.clone()
                }
            })
        })
    }

    fn try_merge_with_selected_parent_cache(
        &self,
        heap: &mut BoundedSizeBlockHeap,
        cache: Option<&Arc<U>>,
        selected_parent: &Hash,
    ) -> bool {
        cache
            .and_then(|cache| {
                cache.get(selected_parent).map(|selected_parent_window| {
                    heap.merge_ancestor_heap(&mut (*selected_parent_window).clone());
                })
            })
            .is_some()
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
    sampling_activation: ForkActivation,
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
        sampling_activation: ForkActivation,
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
            max_difficulty_target,
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
            sampling_activation,
            sampled_difficulty_window_size,
            min_difficulty_window_len.min(sampled_difficulty_window_size),
            difficulty_sample_rate,
            sampled_past_median_time_window_size,
            past_median_time_sample_rate,
        );
        Self { ghostdag_store, headers_store, sampled_window_manager, full_window_manager, sampling_activation }
    }

    fn sampling(&self, ghostdag_data: &GhostdagData) -> bool {
        let sp_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        self.sampling_activation.is_active(sp_daa_score)
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

    // This method is intended to be used to merge the ancestor heap with the current heap.
    fn merge_ancestor_heap(&mut self, ancestor_heap: &mut BlockWindowHeap) {
        self.binary_heap.blocks.append(&mut ancestor_heap.blocks);
        // Below we saturate for cases where ancestor may be close to, the origin, or genesis.
        // Note: this is a no-op if overflow_amount is 0, i.e. because of the saturating sub, the sum of the two heaps is less or equal to the size bound.
        for _ in 0..self.binary_heap.len().saturating_sub(self.size_bound) {
            self.binary_heap.blocks.pop();
        }
    }
}
