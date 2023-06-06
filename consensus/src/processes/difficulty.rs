use crate::model::stores::{
    block_window_cache::BlockWindowHeap,
    ghostdag::{GhostdagData, GhostdagStoreReader},
    headers::HeaderStoreReader,
};
use kaspa_consensus_core::{
    config::params::MIN_DIFFICULTY_WINDOW_LEN,
    errors::difficulty::{DifficultyError, DifficultyResult},
    BlockHashSet, BlueWorkType,
};
use kaspa_math::{Uint256, Uint320};
use std::{
    cmp::{max, Ordering},
    iter::once_with,
    ops::Deref,
    sync::Arc,
};

use super::ghostdag::ordering::SortableBlock;
use itertools::Itertools;

trait DifficultyManagerExtension {
    fn headers_store(&self) -> &dyn HeaderStoreReader;

    #[inline]
    #[must_use]
    fn internal_calc_daa_score(&self, ghostdag_data: &GhostdagData, mergeset_non_daa: &BlockHashSet) -> u64 {
        let sp_daa_score = self.headers_store().get_daa_score(ghostdag_data.selected_parent).unwrap();
        sp_daa_score + (ghostdag_data.mergeset_size() - mergeset_non_daa.len()) as u64
    }

    fn get_difficulty_blocks(&self, window: &BlockWindowHeap) -> Vec<DifficultyBlock> {
        window
            .iter()
            .map(|item| {
                let data = self.headers_store().get_compact_header_data(item.0.hash).unwrap();
                DifficultyBlock { timestamp: data.timestamp, bits: data.bits, sortable_block: item.0.clone() }
            })
            .collect()
    }

    fn internal_estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        // TODO: perhaps move this const
        const MIN_WINDOW_SIZE: usize = 1000;
        let window_size = window.len();
        if window_size < MIN_WINDOW_SIZE {
            return Err(DifficultyError::UnderMinWindowSizeAllowed(window_size, MIN_WINDOW_SIZE));
        }
        let difficulty_blocks = self.get_difficulty_blocks(window);
        let (min_ts, max_ts) = difficulty_blocks.iter().map(|x| x.timestamp).minmax().into_option().unwrap();
        if min_ts == max_ts {
            return Err(DifficultyError::EmptyTimestampRange);
        }
        let window_duration = (max_ts - min_ts) / 1000; // Divided by 1000 to convert milliseconds to seconds
        if window_duration == 0 {
            return Ok(0);
        }

        let (min_blue_work, max_blue_work) =
            difficulty_blocks.iter().map(|x| x.sortable_block.blue_work).minmax().into_option().unwrap();

        Ok(((max_blue_work - min_blue_work) / window_duration).as_u64())
    }

    #[inline]
    fn check_min_difficulty_window_len(difficulty_window_size: usize, min_difficulty_window_len: usize) {
        assert!(
            MIN_DIFFICULTY_WINDOW_LEN <= min_difficulty_window_len && min_difficulty_window_len <= difficulty_window_size,
            "min_difficulty_window_len {} is expected to fit within {}..={}",
            min_difficulty_window_len,
            MIN_DIFFICULTY_WINDOW_LEN,
            difficulty_window_size
        );
    }
}

/// A difficulty manager conforming to the legacy golang implementation
/// based on full, hence un-sampled, windows
#[derive(Clone)]
pub struct FullDifficultyManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_bits: u32,
    difficulty_window_size: usize,
    min_difficulty_window_len: usize,
    target_time_per_block: u64,
}

impl<T: HeaderStoreReader> FullDifficultyManager<T> {
    pub fn new(
        headers_store: Arc<T>,
        genesis_bits: u32,
        difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        target_time_per_block: u64,
    ) -> Self {
        Self::check_min_difficulty_window_len(difficulty_window_size, min_difficulty_window_len);
        Self { headers_store, genesis_bits, difficulty_window_size, min_difficulty_window_len, target_time_per_block }
    }

    pub fn calc_daa_score_and_mergeset_non_daa_blocks<'a>(
        &'a self,
        window: &BlockWindowHeap,
        ghostdag_data: &GhostdagData,
        store: &'a (impl GhostdagStoreReader + ?Sized),
    ) -> (u64, BlockHashSet) {
        // If the window is empty, all the mergeset goes in the non-DAA set, hence a default lowest block with maximum blue work.
        let default_lowest_block = SortableBlock { hash: Default::default(), blue_work: BlueWorkType::MAX };
        let window_lowest_block = window.peek().map(|x| &x.0).unwrap_or_else(|| &default_lowest_block);
        let mergeset_non_daa: BlockHashSet = ghostdag_data
            .ascending_mergeset_without_selected_parent(store)
            .chain(once_with(|| {
                let selected_parent_hash = ghostdag_data.selected_parent;
                SortableBlock { hash: selected_parent_hash, blue_work: store.get_blue_work(selected_parent_hash).unwrap_or_default() }
            }))
            .take_while(|sortable_block| sortable_block < window_lowest_block)
            .map(|sortable_block| sortable_block.hash)
            .collect();

        (self.internal_calc_daa_score(ghostdag_data, &mergeset_non_daa), mergeset_non_daa)
    }

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32 {
        let mut difficulty_blocks = self.get_difficulty_blocks(window);

        // Until there are enough blocks for a valid calculation the difficulty should remain constant.
        if difficulty_blocks.len() < self.min_difficulty_window_len {
            return self.genesis_bits;
        }

        let (min_ts_index, max_ts_index) = difficulty_blocks.iter().position_minmax().into_option().unwrap();

        let min_ts = difficulty_blocks[min_ts_index].timestamp;
        let max_ts = difficulty_blocks[max_ts_index].timestamp;

        // We remove the minimal block because we want the average target for the internal window.
        difficulty_blocks.swap_remove(min_ts_index);

        // We need Uint320 to avoid overflow when summing and multiplying by the window size.
        let difficulty_blocks_len = difficulty_blocks.len() as u64;
        let targets_sum: Uint320 =
            difficulty_blocks.into_iter().map(|diff_block| Uint320::from(Uint256::from_compact_target_bits(diff_block.bits))).sum();
        let average_target = targets_sum / (difficulty_blocks_len);
        let new_target = average_target * max(max_ts - min_ts, 1) / (self.target_time_per_block * difficulty_blocks_len);
        Uint256::try_from(new_target).expect("Expected target should be less than 2^256").compact_target_bits()
    }

    pub fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.internal_estimate_network_hashes_per_second(window)
    }
}

impl<T: HeaderStoreReader> DifficultyManagerExtension for FullDifficultyManager<T> {
    fn headers_store(&self) -> &dyn HeaderStoreReader {
        self.headers_store.deref()
    }
}

/// A difficulty manager implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md),
/// so based on sampled windows
#[derive(Clone)]
pub struct SampledDifficultyManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_bits: u32,
    max_difficulty_target: Uint320,
    difficulty_window_size: usize,
    min_difficulty_window_len: usize,
    difficulty_sample_rate: u64,
    target_time_per_block: u64,
}

impl<T: HeaderStoreReader> SampledDifficultyManager<T> {
    pub fn new(
        headers_store: Arc<T>,
        genesis_bits: u32,
        max_difficulty_target: Uint256,
        difficulty_window_size: usize,
        min_difficulty_window_len: usize,
        difficulty_sample_rate: u64,
        target_time_per_block: u64,
    ) -> Self {
        Self::check_min_difficulty_window_len(difficulty_window_size, min_difficulty_window_len);
        Self {
            headers_store,
            genesis_bits,
            max_difficulty_target: max_difficulty_target.into(),
            difficulty_window_size,
            min_difficulty_window_len,
            difficulty_sample_rate,
            target_time_per_block,
        }
    }

    #[inline]
    #[must_use]
    pub fn difficulty_full_window_size(&self) -> u64 {
        self.difficulty_window_size as u64 * self.difficulty_sample_rate
    }

    /// Returns the DAA window lowest accepted blue score
    #[inline]
    #[must_use]
    pub fn lowest_daa_blue_score(&self, ghostdag_data: &GhostdagData) -> u64 {
        let difficulty_full_window_size = self.difficulty_full_window_size();
        ghostdag_data.blue_score.max(difficulty_full_window_size) - difficulty_full_window_size
    }

    #[inline]
    #[must_use]
    pub fn calc_daa_score(&self, ghostdag_data: &GhostdagData, mergeset_non_daa: &BlockHashSet) -> u64 {
        self.internal_calc_daa_score(ghostdag_data, mergeset_non_daa)
    }

    pub fn calc_daa_score_and_mergeset_non_daa_blocks(
        &self,
        ghostdag_data: &GhostdagData,
        store: &(impl GhostdagStoreReader + ?Sized),
    ) -> (u64, BlockHashSet) {
        let lowest_daa_blue_score = self.lowest_daa_blue_score(ghostdag_data);
        let mergeset_non_daa: BlockHashSet =
            ghostdag_data.unordered_mergeset().filter(|hash| store.get_blue_score(*hash).unwrap() < lowest_daa_blue_score).collect();
        (self.internal_calc_daa_score(ghostdag_data, &mergeset_non_daa), mergeset_non_daa)
    }

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32 {
        // Note: this fn is duplicated (almost, see `* self.difficulty_sample_rate`) in Full and Sampled structs
        // so some alternate calculation can be investigated here.
        let mut difficulty_blocks = self.get_difficulty_blocks(window);

        // Until there are enough blocks for a valid calculation the difficulty should remain constant.
        if difficulty_blocks.len() < self.min_difficulty_window_len {
            return self.genesis_bits;
        }

        let (min_ts_index, max_ts_index) = difficulty_blocks.iter().position_minmax().into_option().unwrap();

        let min_ts = difficulty_blocks[min_ts_index].timestamp;
        let max_ts = difficulty_blocks[max_ts_index].timestamp;

        // We remove the minimal block because we want the average target for the internal window.
        difficulty_blocks.swap_remove(min_ts_index);

        // We need Uint320 to avoid overflow when summing and multiplying by the window size.
        let difficulty_blocks_len = difficulty_blocks.len() as u64;
        let targets_sum: Uint320 =
            difficulty_blocks.into_iter().map(|diff_block| Uint320::from(Uint256::from_compact_target_bits(diff_block.bits))).sum();
        let average_target = targets_sum / difficulty_blocks_len;
        let measured_duration = max(max_ts - min_ts, 1);
        let expected_duration = self.target_time_per_block * self.difficulty_sample_rate * difficulty_blocks_len; // This does differ from FullDifficultyManager version
        let new_target = average_target * measured_duration / expected_duration;
        Uint256::try_from(new_target.min(self.max_difficulty_target)).unwrap().compact_target_bits()
    }

    pub fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.internal_estimate_network_hashes_per_second(window)
    }
}

impl<T: HeaderStoreReader> DifficultyManagerExtension for SampledDifficultyManager<T> {
    fn headers_store(&self) -> &dyn HeaderStoreReader {
        self.headers_store.deref()
    }
}

pub fn calc_work(bits: u32) -> BlueWorkType {
    let target = Uint256::from_compact_target_bits(bits);
    // Source: https://github.com/bitcoin/bitcoin/blob/2e34374bf3e12b37b0c66824a6c998073cdfab01/src/chain.cpp#L131
    // We need to compute 2**256 / (bnTarget+1), but we can't represent 2**256
    // as it's too large for an arith_uint256. However, as 2**256 is at least as large
    // as bnTarget+1, it is equal to ((2**256 - bnTarget - 1) / (bnTarget+1)) + 1,
    // or ~bnTarget / (bnTarget+1) + 1.

    let res = (!target / (target + 1)) + 1;
    res.try_into().expect("Work should not exceed 2**192")
}

#[derive(Eq)]
struct DifficultyBlock {
    timestamp: u64,
    bits: u32,
    sortable_block: SortableBlock,
}

impl PartialEq for DifficultyBlock {
    fn eq(&self, other: &Self) -> bool {
        // If the sortable blocks are equal the timestamps and bits that are associated with the block are equal for sure.
        self.sortable_block == other.sortable_block
    }
}

impl PartialOrd for DifficultyBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DifficultyBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp).then_with(|| self.sortable_block.cmp(&other.sortable_block))
    }
}
