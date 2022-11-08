use crate::model::stores::{block_window_cache::BlockWindowHeap, ghostdag::GhostdagData, headers::HeaderStoreReader};
use consensus_core::{BlockHashSet, BlueWorkType};
use hashes::Hash;
use math::{Uint256, Uint320};
use std::{
    cmp::{max, Ordering},
    sync::Arc,
};

use super::ghostdag::ordering::SortableBlock;
use itertools::Itertools;

#[derive(Clone)]
pub struct DifficultyManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_bits: u32,
    difficulty_adjustment_window_size: usize,
    target_time_per_block: u64,
}

impl<T: HeaderStoreReader> DifficultyManager<T> {
    pub fn new(
        headers_store: Arc<T>,
        genesis_bits: u32,
        difficulty_adjustment_window_size: usize,
        target_time_per_block: u64,
    ) -> Self {
        Self { headers_store, difficulty_adjustment_window_size, genesis_bits, target_time_per_block }
    }

    pub fn calc_daa_score_and_non_daa_mergeset_blocks(
        &self,
        window_hashes: &mut impl ExactSizeIterator<Item = Hash>,
        ghostdag_data: &GhostdagData,
    ) -> (u64, BlockHashSet) {
        let mergeset: BlockHashSet = ghostdag_data.unordered_mergeset().collect();
        let mergeset_daa: BlockHashSet = window_hashes.filter(|h| mergeset.contains(h)).take(mergeset.len()).collect();
        let mergeset_non_daa: BlockHashSet = mergeset.difference(&mergeset_daa).copied().collect();
        let sp_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();

        (sp_daa_score + mergeset_daa.len() as u64, mergeset_non_daa)
    }

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32 {
        let mut difficulty_blocks: Vec<DifficultyBlock> = window
            .iter()
            .map(|item| {
                let data = self.headers_store.get_compact_header_data(item.0.hash).unwrap();
                DifficultyBlock { timestamp: data.timestamp, bits: data.bits, sortable_block: item.0.clone() }
            })
            .collect();

        // Until there are enough blocks for a full block window the difficulty should remain constant.
        if difficulty_blocks.len() < self.difficulty_adjustment_window_size {
            return self.genesis_bits;
        }

        let (min_ts_index, max_ts_index) = difficulty_blocks.iter().position_minmax().into_option().unwrap();

        let min_ts = difficulty_blocks[min_ts_index].timestamp;
        let max_ts = difficulty_blocks[max_ts_index].timestamp;

        // We remove the minimal block because we want the average target for the internal window.
        difficulty_blocks.swap_remove(min_ts_index);

        // We need Uint320 to avoid overflow when summing and multiplying by the window size.
        // TODO: Try to see if we can use U256 instead, by modifying the algorithm.
        let difficulty_blocks_len = difficulty_blocks.len();
        let targets_sum: Uint320 =
            difficulty_blocks.into_iter().map(|diff_block| Uint320::from(Uint256::from_compact_target_bits(diff_block.bits))).sum();
        let average_target = targets_sum / (difficulty_blocks_len as u64);
        let new_target = average_target * max(max_ts - min_ts, 1) / self.target_time_per_block / difficulty_blocks_len as u64;
        Uint256::try_from(new_target).expect("Expected target should be less than 2^256").compact_target_bits()
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
