use crate::model::stores::{block_window_cache::BlockWindowHeap, ghostdag::GhostdagData, headers::HeaderStoreReader};
use consensus_core::BlueWorkType;
use hashes::Hash;
use kaspa_core::*;
use num_bigint::{BigInt, Sign};
use std::{
    cmp::{max, Ordering},
    collections::HashSet,
    sync::Arc,
};

use super::ghostdag::ordering::SortableBlock;
use itertools::{Itertools, MinMaxResult::MinMax};

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

        let mergeset_len = ghostdag_data.mergeset_size();
        let mergeset: HashSet<Hash> = ghostdag_data.unordered_mergeset().collect();

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
        let mut difficulty_blocks: Vec<DifficultyBlock> = window
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

        let (min_ts_index, max_ts_index) =
            extract_enum_value!(difficulty_blocks.iter().position_minmax(), MinMax(a,b) => (a,b));

        let min_ts = difficulty_blocks[min_ts_index].timestamp;
        let max_ts = difficulty_blocks[max_ts_index].timestamp;

        // We remove the minimal block because we want the average target for the internal window.
        difficulty_blocks.swap_remove(min_ts_index);

        let difficulty_blocks_len = difficulty_blocks.len();
        let targets_sum: BigInt = difficulty_blocks
            .into_iter()
            .map(|diff_block| big_from_compact_target(diff_block.bits))
            .sum();
        let average_target = targets_sum / (difficulty_blocks_len as u64);
        let new_target =
            average_target * max(max_ts - min_ts, 1) / self.target_time_per_block / difficulty_blocks_len as u64;
        compact_target_from_big(&new_target)
    }
}

// TODO: Replace BigInt with U256
pub fn big_from_compact_target(bits: u32) -> BigInt {
    // This is a floating-point "compact" encoding originally used by
    // OpenSSL, which satoshi put into consensus code, so we're stuck
    // with it. The exponent needs to have 3 subtracted from it, hence
    // this goofy decoding code:
    let (mant, expt) = {
        let unshifted_expt = bits >> 24;
        if unshifted_expt <= 3 {
            ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt as usize)), 0)
        } else {
            (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
        }
    };

    // The mantissa is signed but may not be negative
    if mant > 0x7FFFFF {
        Default::default()
    } else {
        BigInt::from(mant) << (expt as usize)
    }
}

// TODO: Replace BigInt with U256
/// Computes the target value in float format from BigInt format.
fn compact_target_from_big(value: &BigInt) -> u32 {
    let mut size = (value.bits() + 7) / 8;
    let mut compact = if size <= 3 {
        (value.to_u64_digits().1[0] << (8 * (3 - size))) as u32
    } else {
        let bn = value >> (8 * (size - 3));
        bn.to_u32_digits().1[0]
    };

    if (compact & 0x00800000) != 0 {
        compact >>= 8;
        size += 1;
    }

    compact | (size << 24) as u32
}

pub fn calc_work(bits: u32) -> BlueWorkType {
    let target = big_from_compact_target(bits);
    if target.sign() == Sign::Minus {
        return 0;
    }

    let denominator: BigInt = target + 1;
    let one_lsh_256: BigInt = BigInt::from(1) << 256;
    (one_lsh_256 / denominator).try_into().unwrap()
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
        let res = self.timestamp.cmp(&other.timestamp);
        match res {
            Ordering::Equal => self.sortable_block.cmp(&other.sortable_block),
            _ => res,
        }
    }
}
