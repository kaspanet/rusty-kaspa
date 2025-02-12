use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{time::unix_now, trace, warn};

use super::{mining_rule::MiningRule, ExtraData};

const SYNC_RATE_THRESHOLD: f64 = 0.10;

pub struct SyncRateRule {
    pub use_sync_rate_rule: Arc<AtomicBool>,
}

impl SyncRateRule {
    pub fn new(use_sync_rate_rule: Arc<AtomicBool>) -> Self {
        Self { use_sync_rate_rule }
    }
}

impl MiningRule for SyncRateRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData) {
        let expected_blocks = (extra_data.elapsed_time.as_millis() as u64) / extra_data.target_time_per_block;
        let received_blocks = delta.body_counts.max(delta.header_counts);
        let rate: f64 = (received_blocks as f64) / (expected_blocks as f64);

        // Finality point is considered "recent" if it is within 3 finality durations from the current time
        let is_finality_recent = extra_data.finality_point_timestamp >= unix_now().saturating_sub(extra_data.finality_duration * 3);

        trace!(
            "Sync rate: {:.2} | Finality point recent: {} | Elapsed time: {}s | Connected: {} | Found/Expected blocks: {}/{}",
            rate,
            is_finality_recent,
            extra_data.elapsed_time.as_secs(),
            extra_data.has_sufficient_peer_connectivity,
            delta.body_counts,
            expected_blocks,
        );

        if is_finality_recent && rate < SYNC_RATE_THRESHOLD {
            // if sync rate rule conditions are met:
            if let Ok(false) = self.use_sync_rate_rule.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) {
                warn!("Sync rate {:.2} is below threshold: {}", rate, SYNC_RATE_THRESHOLD);
            }
        } else {
            // else when sync rate conditions are not met:
            if let Ok(true) = self.use_sync_rate_rule.compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed) {
                if !is_finality_recent {
                    warn!("Sync rate {:.2} recovered: {} by entering IBD", rate, SYNC_RATE_THRESHOLD);
                } else {
                    warn!("Sync rate {:.2} recovered: {}", rate, SYNC_RATE_THRESHOLD);
                }
            } else if !is_finality_recent {
                trace!("Finality period is old. Timestamp: {}. Sync rate: {:.2}", extra_data.finality_point_timestamp, rate);
            }
        }
    }
}
