use crate::model::stores::{block_window_cache::BlockWindowHeap, headers::HeaderStoreReader};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_hashes::Hash;
use std::sync::Arc;

/// A past median manager conforming to the legacy golang implementation
/// based on full, hence un-sampled, windows
#[derive(Clone)]
pub struct FullPastMedianTimeManager<T: HeaderStoreReader> {
    headers_store: Arc<T>,
    genesis_timestamp: u64,
}

impl<T: HeaderStoreReader> FullPastMedianTimeManager<T> {
    pub fn new(headers_store: Arc<T>, genesis_timestamp: u64) -> Self {
        Self { headers_store, genesis_timestamp }
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

/// A past median time manager implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md),
/// so based on sampled windows
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

        /*

           [Crescendo]: In the first moments post activation the median time window will be empty or smaller than expected.
                        Which means that past median time will be closer to current time and less flexible. This is ok since
                        BBT makes sure to respect this lower bound. The following alternatives were considered and ruled out:

                            1. fill the window with non activated blocks as well, this means the sampled window will go 10x
                               time back (~45 minutes), so the timestamp for the first blocks post activation can go ~22
                               minutes back (if abused). The result for DAA can be further temporary acceleration beyond
                               the new desired BPS (window duration will be much longer than expected hence difficulty will
                               go down further).

                            2. sampling the window before and after the activation with different corresponding sample rates. This approach
                               is ruled out due to complexity, and because the proposed (simpler) solution has no significant drawbacks.

                        With the proposed solution, the worst case scenario can be forcing the last blocks pre-activation to a timestamp
                        which is timestamp_deviation_tolerance seconds in the future (~2 minutes), which will force the first blocks post
                        activation to this timestamp as well. However, this will only slightly smooth out the block rate transition.
        */

        if window.is_empty() {
            // [Crescendo]: this indicates we are in the few seconds post activation where the window is
            // still empty, simply take the selected parent timestamp
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
