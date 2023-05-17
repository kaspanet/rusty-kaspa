use crate::model::stores::{block_window_cache::BlockWindowHeap, headers::HeaderStoreReader};
use kaspa_consensus_core::errors::block::RuleError;
use std::sync::Arc;

#[derive(Clone)]
pub struct PastMedianTimeManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    past_median_time_window_size: usize,
    past_median_time_sample_rate: u64,
    genesis_timestamp: u64,
}

impl<T: HeaderStoreReader> PastMedianTimeManager<T> {
    pub fn new(
        headers_store: Arc<T>,
        past_median_time_window_size: usize,
        past_median_time_sample_rate: u64,
        genesis_timestamp: u64,
    ) -> Self {
        Self { headers_store, past_median_time_window_size, past_median_time_sample_rate, genesis_timestamp }
    }

    pub fn calc_past_median_time(&self, window: &BlockWindowHeap) -> Result<u64, RuleError> {
        if window.is_empty() {
            return Ok(self.genesis_timestamp);
        }

        let mut window_timestamps: Vec<u64> =
            window.iter().map(|item| self.headers_store.get_timestamp(item.0.hash).unwrap()).collect();
        window_timestamps.sort_unstable(); // This is deterministic because we sort u64
        Ok(window_timestamps[window_timestamps.len() / 2])
    }
}
