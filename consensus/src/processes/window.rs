use crate::{
    model::stores::{
        block_window_cache::{BlockWindowCacheReader, BlockWindowCacheWriter, BlockWindowHeap, WindowOrigin},
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
    DifficultyWindow,
    MedianTimeWindow,
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
    fn calc_past_median_time_for_known_hash(&self, hash: Hash) -> Result<u64, RuleError>;
    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64>;
    fn window_size(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> usize;
    fn sample_rate(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> u64;

    /// Returns the full consecutive sub-DAG containing all blocks required to restore the (possibly sampled) window.
    fn consecutive_cover_for_window(&self, ghostdag_data: Arc<GhostdagData>, window: &BlockWindowHeap) -> Vec<Hash>;
}

trait AffiliatedWindowCacheReader {
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>>;
}

/// A local wrapper over an (optional) block window cache which filters cache hits based on a pre-specified window origin
struct AffiliatedWindowCache<'a, U: BlockWindowCacheReader> {
    /// The inner underlying cache
    inner: Option<&'a Arc<U>>,
    /// The affiliated origin (sampled vs. full)
    origin: WindowOrigin,
}

impl<'a, U: BlockWindowCacheReader> AffiliatedWindowCache<'a, U> {
    fn new(inner: Option<&'a Arc<U>>, origin: WindowOrigin) -> Self {
        Self { inner, origin }
    }
}

impl<U: BlockWindowCacheReader> AffiliatedWindowCacheReader for AffiliatedWindowCache<'_, U> {
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>> {
        // Only return the cached window if it originates from the affiliated origin
        self.inner.and_then(|cache| cache.get(hash, self.origin))
    }
}

/// A window manager conforming (indirectly) to the legacy golang implementation
/// based on full, hence un-sampled, windows
#[derive(Clone)]
pub struct FullWindowManager<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader> {
    genesis_hash: Hash,
    ghostdag_store: Arc<T>,
    block_window_cache_for_difficulty: Arc<U>,
    block_window_cache_for_past_median_time: Arc<U>,
    difficulty_window_size: usize,
    past_median_time_window_size: usize,
    difficulty_manager: FullDifficultyManager<V>,
    past_median_time_manager: FullPastMedianTimeManager<V>,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader> FullWindowManager<T, U, V> {
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

        let inner_cache = if window_size == self.difficulty_window_size {
            Some(&self.block_window_cache_for_difficulty)
        } else if window_size == self.past_median_time_window_size {
            Some(&self.block_window_cache_for_past_median_time)
        } else {
            None
        };
        // Wrap the inner cache with a cache affiliated with this origin (WindowOrigin::Full).
        // This is crucial for hardfork times where the DAA mechanism changes thereby invalidating cache entries
        // originating from the prior mechanism
        let cache = AffiliatedWindowCache::new(inner_cache, WindowOrigin::Full);

        if let Some(selected_parent_binary_heap) = cache.get(&ghostdag_data.selected_parent) {
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

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader> WindowManager
    for FullWindowManager<T, U, V>
{
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        self.build_block_window(ghostdag_data, window_type)
    }

    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow {
        let (daa_score, mergeset_non_daa) =
            self.difficulty_manager.calc_daa_score_and_mergeset_non_daa_blocks(&window, ghostdag_data, self.ghostdag_store.deref());
        DaaWindow::new(window, daa_score, mergeset_non_daa)
    }

    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::DifficultyWindow)?;
        Ok(self.calc_daa_window(ghostdag_data, window))
    }

    fn calculate_difficulty_bits(&self, _high_ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        self.difficulty_manager.calculate_difficulty_bits(&daa_window.window)
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::MedianTimeWindow)?;
        let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
        Ok((past_median_time, window))
    }

    fn calc_past_median_time_for_known_hash(&self, hash: Hash) -> Result<u64, RuleError> {
        if let Some(window) = self.block_window_cache_for_past_median_time.get(&hash, WindowOrigin::Full) {
            let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
            Ok(past_median_time)
        } else {
            let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();
            let (past_median_time, window) = self.calc_past_median_time(&ghostdag_data)?;
            self.block_window_cache_for_past_median_time.insert(hash, window);
            Ok(past_median_time)
        }
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.difficulty_manager.estimate_network_hashes_per_second(&window)
    }

    fn window_size(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match window_type {
            WindowType::DifficultyWindow => self.difficulty_window_size,
            WindowType::MedianTimeWindow => self.past_median_time_window_size,
            WindowType::VaryingWindow(size) => size,
        }
    }

    fn sample_rate(&self, _ghostdag_data: &GhostdagData, _window_type: WindowType) -> u64 {
        1
    }

    fn consecutive_cover_for_window(&self, _ghostdag_data: Arc<GhostdagData>, window: &BlockWindowHeap) -> Vec<Hash> {
        assert_eq!(WindowOrigin::Full, window.origin());
        window.iter().map(|b| b.0.hash).collect()
    }
}

type DaaStatus = Option<(u64, BlockHashSet)>;

enum SampledBlock {
    Sampled(SortableBlock),
    NonDaa(Hash),
}

/// A sampled window manager implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md)
#[derive(Clone)]
pub struct SampledWindowManager<
    T: GhostdagStoreReader,
    U: BlockWindowCacheReader + BlockWindowCacheWriter,
    V: HeaderStoreReader,
    W: DaaStoreReader,
> {
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

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader, W: DaaStoreReader>
    SampledWindowManager<T, U, V, W>
{
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

        let inner_cache = match window_type {
            WindowType::DifficultyWindow => Some(&self.block_window_cache_for_difficulty),
            WindowType::MedianTimeWindow => Some(&self.block_window_cache_for_past_median_time),
            WindowType::VaryingWindow(_) => None,
        };
        // Wrap the inner cache with a cache affiliated with this origin (WindowOrigin::Sampled).
        // This is crucial for hardfork times where the DAA mechanism changes thereby invalidating cache entries
        // originating from the prior mechanism
        let cache = AffiliatedWindowCache::new(inner_cache, WindowOrigin::Sampled);

        let selected_parent_blue_work = self.ghostdag_store.get_blue_work(ghostdag_data.selected_parent).unwrap();

        // Try to initialize the window from the cache directly
        if let Some(res) = self.try_init_from_cache(
            window_size,
            sample_rate,
            &cache,
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
            if self.try_merge_with_selected_parent_cache(&mut window_heap, &cache, &current_ghostdag.selected_parent) {
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
        cache: &impl AffiliatedWindowCacheReader,
        ghostdag_data: &GhostdagData,
        selected_parent_blue_work: BlueWorkType,
        mergeset_non_daa_inserter: Option<impl FnMut(Hash)>,
    ) -> Option<Arc<BlockWindowHeap>> {
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
    }

    fn try_merge_with_selected_parent_cache(
        &self,
        heap: &mut BoundedSizeBlockHeap,
        cache: &impl AffiliatedWindowCacheReader,
        selected_parent: &Hash,
    ) -> bool {
        cache
            .get(selected_parent)
            .map(|selected_parent_window| {
                heap.merge_ancestor_heap(&mut (*selected_parent_window).clone());
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

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader, W: DaaStoreReader> WindowManager
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
        let window = self.build_block_window(ghostdag_data, WindowType::DifficultyWindow, |hash| {
            mergeset_non_daa.insert(hash);
        })?;
        let daa_score = self.difficulty_manager.calc_daa_score(ghostdag_data, &mergeset_non_daa);
        Ok(DaaWindow::new(window, daa_score, mergeset_non_daa))
    }

    fn calculate_difficulty_bits(&self, _high_ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        self.difficulty_manager.calculate_difficulty_bits(&daa_window.window)
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        let window = self.block_window(ghostdag_data, WindowType::MedianTimeWindow)?;
        let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
        Ok((past_median_time, window))
    }

    fn calc_past_median_time_for_known_hash(&self, hash: Hash) -> Result<u64, RuleError> {
        if let Some(window) = self.block_window_cache_for_past_median_time.get(&hash, WindowOrigin::Sampled) {
            let past_median_time = self.past_median_time_manager.calc_past_median_time(&window)?;
            Ok(past_median_time)
        } else {
            let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();
            let (past_median_time, window) = self.calc_past_median_time(&ghostdag_data)?;
            self.block_window_cache_for_past_median_time.insert(hash, window);
            Ok(past_median_time)
        }
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.difficulty_manager.estimate_network_hashes_per_second(&window)
    }

    fn window_size(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match window_type {
            WindowType::DifficultyWindow => self.difficulty_window_size,
            WindowType::MedianTimeWindow => self.past_median_time_window_size,
            WindowType::VaryingWindow(size) => size,
        }
    }

    fn sample_rate(&self, _ghostdag_data: &GhostdagData, window_type: WindowType) -> u64 {
        match window_type {
            WindowType::DifficultyWindow => self.difficulty_sample_rate,
            WindowType::MedianTimeWindow => self.past_median_time_sample_rate,
            WindowType::VaryingWindow(_) => 1,
        }
    }

    fn consecutive_cover_for_window(&self, mut ghostdag: Arc<GhostdagData>, window: &BlockWindowHeap) -> Vec<Hash> {
        assert_eq!(WindowOrigin::Sampled, window.origin());

        // In the sampled case, the sampling logic relies on DAA indexes which can only be calculated correctly if the full
        // mergesets covering all sampled blocks are sent.

        // Tracks the window blocks to make sure we visit all blocks
        let mut unvisited: BlockHashSet = window.iter().map(|b| b.0.hash).collect();
        let capacity_estimate = window.len() * self.difficulty_sample_rate as usize;
        // The full consecutive window covering all sampled window blocks and the full mergesets containing them
        let mut cover = Vec::with_capacity(capacity_estimate);
        while !unvisited.is_empty() {
            assert!(!ghostdag.selected_parent.is_origin(), "unvisited still not empty");
            // TODO (relaxed): a possible optimization here is to iterate in the same order as
            // sampled_mergeset_iterator (descending_mergeset) and to break once all samples from
            // this mergeset are reached.
            // * Why is this sufficient? bcs we still send the prefix of the mergeset required for
            //                           obtaining the DAA index for all sampled blocks.
            // * What's the benefit? This might exclude deeply merged blocks which in turn will help
            //                       reducing the number of trusted blocks sent to a fresh syncing peer.
            for merged in ghostdag.unordered_mergeset() {
                cover.push(merged);
                unvisited.remove(&merged);
            }
            if unvisited.is_empty() {
                break;
            }
            ghostdag = self.ghostdag_store.get_data(ghostdag.selected_parent).unwrap();
        }
        cover
    }
}

/// A window manager handling either full (un-sampled) or sampled windows depending on an activation DAA score
///
/// See [FullWindowManager] and [SampledWindowManager]
#[derive(Clone)]
pub struct DualWindowManager<
    T: GhostdagStoreReader,
    U: BlockWindowCacheReader + BlockWindowCacheWriter,
    V: HeaderStoreReader,
    W: DaaStoreReader,
> {
    ghostdag_store: Arc<T>,
    headers_store: Arc<V>,
    sampling_activation: ForkActivation,
    full_window_manager: FullWindowManager<T, U, V>,
    sampled_window_manager: SampledWindowManager<T, U, V, W>,
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader, W: DaaStoreReader>
    DualWindowManager<T, U, V, W>
{
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

    /// Checks whether sampling mode was activated based on the selected parent (internally checking its DAA score)
    pub(crate) fn sampling(&self, selected_parent: Hash) -> bool {
        let sp_daa_score = self.headers_store.get_daa_score(selected_parent).unwrap();
        self.sampling_activation.is_active(sp_daa_score)
    }
}

impl<T: GhostdagStoreReader, U: BlockWindowCacheReader + BlockWindowCacheWriter, V: HeaderStoreReader, W: DaaStoreReader> WindowManager
    for DualWindowManager<T, U, V, W>
{
    fn block_window(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> Result<Arc<BlockWindowHeap>, RuleError> {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.block_window(ghostdag_data, window_type),
            false => self.full_window_manager.block_window(ghostdag_data, window_type),
        }
    }

    fn calc_daa_window(&self, ghostdag_data: &GhostdagData, window: Arc<BlockWindowHeap>) -> DaaWindow {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.calc_daa_window(ghostdag_data, window),
            false => self.full_window_manager.calc_daa_window(ghostdag_data, window),
        }
    }

    fn block_daa_window(&self, ghostdag_data: &GhostdagData) -> Result<DaaWindow, RuleError> {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.block_daa_window(ghostdag_data),
            false => self.full_window_manager.block_daa_window(ghostdag_data),
        }
    }

    fn calculate_difficulty_bits(&self, ghostdag_data: &GhostdagData, daa_window: &DaaWindow) -> u32 {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.calculate_difficulty_bits(ghostdag_data, daa_window),
            false => self.full_window_manager.calculate_difficulty_bits(ghostdag_data, daa_window),
        }
    }

    fn calc_past_median_time(&self, ghostdag_data: &GhostdagData) -> Result<(u64, Arc<BlockWindowHeap>), RuleError> {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.calc_past_median_time(ghostdag_data),
            false => self.full_window_manager.calc_past_median_time(ghostdag_data),
        }
    }

    fn calc_past_median_time_for_known_hash(&self, hash: Hash) -> Result<u64, RuleError> {
        match self.sampling(self.ghostdag_store.get_selected_parent(hash).unwrap()) {
            true => self.sampled_window_manager.calc_past_median_time_for_known_hash(hash),
            false => self.full_window_manager.calc_past_median_time_for_known_hash(hash),
        }
    }

    fn estimate_network_hashes_per_second(&self, window: Arc<BlockWindowHeap>) -> DifficultyResult<u64> {
        self.sampled_window_manager.estimate_network_hashes_per_second(window)
    }

    fn window_size(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> usize {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.window_size(ghostdag_data, window_type),
            false => self.full_window_manager.window_size(ghostdag_data, window_type),
        }
    }

    fn sample_rate(&self, ghostdag_data: &GhostdagData, window_type: WindowType) -> u64 {
        match self.sampling(ghostdag_data.selected_parent) {
            true => self.sampled_window_manager.sample_rate(ghostdag_data, window_type),
            false => self.full_window_manager.sample_rate(ghostdag_data, window_type),
        }
    }

    fn consecutive_cover_for_window(&self, ghostdag_data: Arc<GhostdagData>, window: &BlockWindowHeap) -> Vec<Hash> {
        match window.origin() {
            WindowOrigin::Sampled => self.sampled_window_manager.consecutive_cover_for_window(ghostdag_data, window),
            WindowOrigin::Full => self.full_window_manager.consecutive_cover_for_window(ghostdag_data, window),
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
