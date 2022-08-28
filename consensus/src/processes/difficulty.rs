use crate::model::stores::{ghostdag::GhostdagData, headers::HeaderStoreReader};
use consensus_core::BlueWorkType;
use hashes::Hash;
use std::{collections::HashSet, sync::Arc};

pub struct DifficultyManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_bits: u32,
    difficulty_adjustment_window_size: usize,
}

impl<T: HeaderStoreReader> DifficultyManager<T> {
    pub fn new(headers_store: Arc<T>, genesis_bits: u32, difficulty_adjustment_window_size: usize) -> Self {
        Self { headers_store, difficulty_adjustment_window_size, genesis_bits }
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

    pub fn calculate_difficulty_bits(&self, window_hashes: &mut impl ExactSizeIterator<Item = Hash>) -> u32 {
        // Until there are enough blocks for a full block window the difficulty should remain constant.
        if window_hashes.len() < self.difficulty_adjustment_window_size {
            return self.genesis_bits;
        }

        let window_headers = window_hashes.map(|hash| self.headers_store.get_header(hash));
        0 // TODO: Calculate real difficulty
    }
}

struct DifficultyBlock {
    timestamp: u64,
    bits: u32,
    hash: Hash,
    blue_work: BlueWorkType,
}
