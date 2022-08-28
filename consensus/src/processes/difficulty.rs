use crate::model::stores::{block_window_cache::BlockWindowHeap, ghostdag::GhostdagData, headers::HeaderStoreReader};
use hashes::Hash;
use std::{
    cmp::{max, Ordering},
    collections::HashSet,
    sync::Arc,
};

use super::ghostdag::ordering::SortableBlock;

#[derive(Clone)]
pub struct DifficultyManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_bits: u32,
    difficulty_adjustment_window_size: usize,
    target_time_per_block: u64,
}

impl<T: HeaderStoreReader> DifficultyManager<T> {
    pub fn new(
        headers_store: Arc<T>, genesis_bits: u32, difficulty_adjustment_window_size: usize, target_time_per_block: u64,
    ) -> Self {
        Self { headers_store, difficulty_adjustment_window_size, genesis_bits, target_time_per_block }
    }

    pub fn calc_daa_score_and_added_blocks(
        &self, window_hashes: &mut impl ExactSizeIterator<Item = Hash>, ghostdag_data: &GhostdagData,
    ) -> (u64, Vec<Hash>) {
        if window_hashes.len() == 0 {
            return (0, Vec::new());
        }

        let mergeset_len = ghostdag_data.mergeset_blues.len() + ghostdag_data.mergeset_reds.len();
        let mut mergeset = HashSet::with_capacity(mergeset_len);
        for blue in ghostdag_data.mergeset_blues.iter() {
            mergeset.insert(blue);
        }
        for red in ghostdag_data.mergeset_reds.iter() {
            mergeset.insert(red);
        }

        let mut daa_added_blocks = Vec::with_capacity(mergeset_len);
        for hash in window_hashes {
            if mergeset.contains(&hash) {
                daa_added_blocks.push(hash);
                if daa_added_blocks.len() == mergeset_len {
                    break;
                }
            }
        }

        let sp_daa_score = self
            .headers_store
            .get_daa_score(ghostdag_data.selected_parent)
            .unwrap();

        (sp_daa_score + daa_added_blocks.len() as u64, daa_added_blocks)
    }

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap) -> u32 {
        let difficulty_blocks: Vec<DifficultyBlock> = window
            .iter()
            .map(|item| {
                let data = self
                    .headers_store
                    .get_compact_header_data(item.0.hash)
                    .unwrap();
                DifficultyBlock { timestamp: data.timestamp, bits: data.bits, sortable_block: item.0.clone() }
            })
            .collect();

        // Until there are enough blocks for a full block window the difficulty should remain constant.
        if difficulty_blocks.len() < self.difficulty_adjustment_window_size {
            return self.genesis_bits;
        }

        let (min_ts_index, min_diff_block) = difficulty_blocks
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.cmp(b))
            .unwrap();
        let min_ts = min_diff_block.timestamp;
        let max_diff_block = difficulty_blocks.iter().max().unwrap();
        let max_ts = max_diff_block.timestamp;

        let targets_len = difficulty_blocks.len() - 1;

        let targets = difficulty_blocks
            .into_iter()
            .skip(min_ts_index)
            .map(|diff_block| compact_to_target(diff_block.bits));

        let targets_sum: u64 = targets.sum();
        let average_target: u64 = targets_sum / targets_len as u64;
        let new_target = average_target * max(max_ts - min_ts, 1) / self.target_time_per_block / targets_len as u64;
        0 // TODO: Calculate real difficulty
    }
}

// TODO: This should be replaced with u256_from_compact_target once the whole u256 functionality is implemented.
fn compact_to_target(bits: u32) -> u64 {
    1
}

#[derive(Eq)]
struct DifficultyBlock {
    timestamp: u64,
    bits: u32,
    sortable_block: SortableBlock,
}

impl PartialEq for DifficultyBlock {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.sortable_block == other.sortable_block
    }
}

impl PartialOrd for DifficultyBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DifficultyBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        let res = self.timestamp.cmp(&other.timestamp);
        match res {
            Ordering::Equal => self.sortable_block.cmp(&other.sortable_block),
            _ => res,
        }
    }
}
