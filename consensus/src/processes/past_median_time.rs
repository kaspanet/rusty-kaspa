use crate::model::stores::{block_window_cache::BlockWindowHeap, headers::HeaderStoreReader};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_hashes::Hash;
use std::sync::Arc;

/// A past median time manager based on sampled block windows, implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md)
#[derive(Clone)]
pub struct SampledPastMedianTimeManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_timestamp: u64,
}

impl<T: HeaderStoreReader> SampledPastMedianTimeManager<T> {
    pub fn new(headers_store: Arc<T>, genesis_timestamp: u64) -> Self {
        Self { headers_store, genesis_timestamp }
    }

    pub fn calc_past_median_time(&self, window: &BlockWindowHeap, selected_parent: Hash) -> Result<u64, RuleError> {
        // The past median time is actually calculated taking the average of the 11 values closest to the center
        // of the sorted timestamps
        const AVERAGE_FRAME_SIZE: usize = 11;

        if window.is_empty() {
            // This indicates we are in the few seconds post hardfork/network activation where the window is
            // still empty, so we simply take the selected parent timestamp
            return Ok(self.headers_store.get_timestamp(selected_parent).unwrap());
        }

        let mut window_timestamps: Vec<u64> =
            window.iter().map(|item| self.headers_store.get_timestamp(item.0.hash).unwrap()).collect();
        window_timestamps.sort_unstable(); // This is deterministic because we sort u64
        let avg_frame_size = window_timestamps.len().min(AVERAGE_FRAME_SIZE);
        // Define the slice so that the average is the highest among the 2 possible solutions in case of an even frame size
        let ending_index = (window_timestamps.len() + avg_frame_size).div_ceil(2);
        let timestamp = (window_timestamps[ending_index - avg_frame_size..ending_index].iter().sum::<u64>()
            + avg_frame_size as u64 / 2)
            / avg_frame_size as u64;
        Ok(timestamp)
    }
}
