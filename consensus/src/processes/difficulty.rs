use crate::model::stores::{
    block_window_cache::BlockWindowHeap,
    ghostdag::{GhostdagData, GhostdagStoreReader},
    headers::HeaderStoreReader,
};
use kaspa_consensus_core::{
    config::params::MAX_DIFFICULTY_TARGET_AS_F64,
    errors::difficulty::{DifficultyError, DifficultyResult},
    BlockHashSet, BlueWorkType, MAX_WORK_LEVEL,
};
use kaspa_core::{info, log::CRESCENDO_KEYWORD};
use kaspa_hashes::Hash;
use kaspa_math::{Uint256, Uint320};
use std::{
    cmp::{max, Ordering},
    ops::Deref,
    sync::{
        atomic::{AtomicU8, Ordering as AtomicOrdering},
        Arc,
    },
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
    fn check_min_difficulty_window_size(difficulty_window_size: usize, min_difficulty_window_size: usize) {
        assert!(
            min_difficulty_window_size <= difficulty_window_size,
            "min_difficulty_window_size {} is expected to be <= difficulty_window_size {}",
            min_difficulty_window_size,
            difficulty_window_size
        );
    }
}

#[derive(Clone)]
struct CrescendoLogger {
    steps: Arc<AtomicU8>,
}

impl CrescendoLogger {
    fn new() -> Self {
        Self { steps: Arc::new(AtomicU8::new(Self::ACTIVATE)) }
    }

    const ACTIVATE: u8 = 0;
    const DYNAMIC: u8 = 1;
    const FULL: u8 = 2;

    pub fn report_activation_progress(&self, step: u8) -> bool {
        if self.steps.compare_exchange(step, step + 1, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst).is_ok() {
            match step {
                Self::ACTIVATE => {
                    info!(target: CRESCENDO_KEYWORD,
                        r#"
        ____                                  _             
       / ___|_ __ ___  ___  ___ ___ _ __   __| | ___        
      | |   | '__/ _ \/ __|/ __/ _ \ '_ \ / _` |/ _ \       
      | |___| | |  __/\__ \ (_|  __/ | | | (_| | (_) |      
       \____|_|  \___||___/\___\___|_| |_|\__,_|\___/       
  _ _                       __      _  ___  _               
 / | |__  _ __  ___         \ \    / |/ _ \| |__  _ __  ___ 
 | | '_ \| '_ \/ __|    _____\ \   | | | | | '_ \| '_ \/ __|
 | | |_) | |_) \__ \   |_____/ /   | | |_| | |_) | |_) \__ \
 |_|_.__/| .__/|___/        /_/    |_|\___/|_.__/| .__/|___/
         |_|                                     |_|    
"#
                    );
                    info!(target: CRESCENDO_KEYWORD, "[Crescendo] Accelerating block rate 10 fold")
                }
                Self::DYNAMIC => {}
                Self::FULL => {}
                _ => {}
            }
            true
        } else {
            false
        }
    }
}

fn hash_suffix(n: f64) -> (f64, &'static str) {
    match n {
        n if n < 1_000.0 => (n, "hash/block"),
        n if n < 1_000_000.0 => (n / 1_000.0, "Khash/block"),
        n if n < 1_000_000_000.0 => (n / 1_000_000.0, "Mhash/block"),
        n if n < 1_000_000_000_000.0 => (n / 1_000_000_000.0, "Ghash/block"),
        n if n < 1_000_000_000_000_000.0 => (n / 1_000_000_000_000.0, "Thash/block"),
        n if n < 1_000_000_000_000_000_000.0 => (n / 1_000_000_000_000_000.0, "Phash/block"),
        n => (n / 1_000_000_000_000_000_000.0, "Ehash/block"),
    }
}

fn difficulty_desc(target: Uint320) -> String {
    let difficulty = MAX_DIFFICULTY_TARGET_AS_F64 / target.as_f64();
    let hashrate = difficulty * 2.0;
    let (rate, suffix) = hash_suffix(hashrate);
    format!("{:.2} {}", rate, suffix)
}

/// A difficulty manager based on sampled block windows, implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md)
#[derive(Clone)]
pub struct SampledDifficultyManager<T: HeaderStoreReader, U: GhostdagStoreReader> {
    headers_store: Arc<T>,
    ghostdag_store: Arc<U>,
    genesis_hash: Hash,
    genesis_bits: u32,
    max_difficulty_target: Uint320,
    difficulty_window_size: usize,
    min_difficulty_window_size: usize,
    difficulty_sample_rate: u64,
    target_time_per_block: u64,
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> SampledDifficultyManager<T, U> {
    pub fn new(
        headers_store: Arc<T>,
        ghostdag_store: Arc<U>,
        genesis_hash: Hash,
        genesis_bits: u32,
        max_difficulty_target: Uint256,
        difficulty_window_size: usize,
        min_difficulty_window_size: usize,
        difficulty_sample_rate: u64,
        target_time_per_block: u64,
    ) -> Self {
        Self::check_min_difficulty_window_size(difficulty_window_size, min_difficulty_window_size);
        Self {
            headers_store,
            ghostdag_store,
            genesis_hash,
            genesis_bits,
            max_difficulty_target: max_difficulty_target.into(),
            difficulty_window_size,
            min_difficulty_window_size,
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

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap, ghostdag_data: &GhostdagData) -> u32 {
        let mut difficulty_blocks = self.get_difficulty_blocks(window);

        // Until there are enough blocks for a valid calculation the difficulty should remain constant.
        if difficulty_blocks.len() < self.min_difficulty_window_size {
            let selected_parent = ghostdag_data.selected_parent;
            if selected_parent == self.genesis_hash {
                return self.genesis_bits;
            }

            // We will use the selected parent as a source for the difficulty bits
            return self.headers_store.get_bits(selected_parent).unwrap();
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

        Uint256::try_from(new_target.min(self.max_difficulty_target)).expect("max target < Uint256::MAX").compact_target_bits()
    }

    pub fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.internal_estimate_network_hashes_per_second(window)
    }
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> DifficultyManagerExtension for SampledDifficultyManager<T, U> {
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

pub fn level_work(level: u8, max_block_level: u8) -> BlueWorkType {
    // Need to make a special condition for level 0 to ensure true work is always used
    if level == 0 {
        return 0.into();
    }
    // We use 256 here so the result corresponds to the work at the level from calc_level_from_pow
    let exp = (level as u32) + 256 - (max_block_level as u32);
    BlueWorkType::from_u64(1) << exp.min(MAX_WORK_LEVEL as u32)
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

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{BlockLevel, BlueWorkType, MAX_WORK_LEVEL};
    use kaspa_math::{Uint256, Uint320};
    use kaspa_pow::calc_level_from_pow;

    use crate::processes::difficulty::{calc_work, level_work};
    use kaspa_utils::hex::ToHex;

    #[test]
    fn test_target_levels() {
        let max_block_level: BlockLevel = 225;
        for level in 1..=max_block_level {
            // required pow for level
            let level_target = (Uint320::from_u64(1) << (max_block_level - level).max(MAX_WORK_LEVEL) as u32) - Uint320::from_u64(1);
            let level_target = Uint256::from_be_bytes(level_target.to_be_bytes()[8..40].try_into().unwrap());
            let calculated_level = calc_level_from_pow(level_target, max_block_level);

            let true_level_work = calc_work(level_target.compact_target_bits());
            let calc_level_work = level_work(level, max_block_level);

            // A "good enough" estimate of level work is within 1% diff from work with actual level target
            // It's hard to calculate percentages with these large numbers, so to get around using floats
            // we multiply the difference by 100. if the result is <= the calc_level_work it means
            // difference must have been less than 1%
            let (percent_diff, overflowed) = (true_level_work - calc_level_work).overflowing_mul(BlueWorkType::from_u64(100));
            let is_good_enough = percent_diff <= calc_level_work;

            println!("Level {}:", level);
            println!(
                "    data | {} | {} | {} / {} |",
                level_target.compact_target_bits(),
                level_target.bits(),
                calculated_level,
                max_block_level
            );
            println!("    pow  | {}", level_target.to_hex());
            println!("    work | 0000000000000000{}", true_level_work.to_hex());
            println!("  lvwork | 0000000000000000{}", calc_level_work.to_hex());
            println!(" diff<1% | {}", !overflowed && (is_good_enough));

            assert!(is_good_enough);
        }
    }

    #[test]
    fn test_base_level_work() {
        // Expect that at level 0, the level work is always 0
        assert_eq!(BlueWorkType::from(0), level_work(0, 255));
    }
}
