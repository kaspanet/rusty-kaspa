use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{time::unix_now, trace, warn};

use crate::rule_engine::SNAPSHOT_INTERVAL;

use super::{mining_rule::MiningRule, ExtraData};

// within a 5 minute period, we expect sync rate
const SYNC_RATE_THRESHOLD: f64 = 0.90;
// number of samples you expect in a 5 minute interval, sampled every 10s
const SYNC_RATE_WINDOW_MAX_SIZE: usize = 5 * 60 / (SNAPSHOT_INTERVAL as usize);
// number of samples required before considering this rule. This allows using the sync rate rule
// even before the full window size is reached. Represents the number of samples in 1 minute
const SYNC_RATE_WINDOW_MIN_THRESHOLD: usize = 60 / (SNAPSHOT_INTERVAL as usize);

pub struct SyncRateRule {
    pub use_sync_rate_rule: Arc<AtomicBool>,
    sync_rate_samples: RwLock<VecDeque<(u64, u64)>>,
    total_expected_blocks: AtomicU64,
    total_received_blocks: AtomicU64,
}

impl SyncRateRule {
    pub fn new(use_sync_rate_rule: Arc<AtomicBool>) -> Self {
        Self {
            use_sync_rate_rule,
            sync_rate_samples: RwLock::new(VecDeque::new()),
            total_expected_blocks: AtomicU64::new(0),
            total_received_blocks: AtomicU64::new(0),
        }
    }

    /// Adds current observation of received and expected blocks to the sample window, and removes
    /// old samples. Returns true if there are enough samples in the window to start triggering the
    /// sync rate rule.
    fn update_sync_rate_window(&self, received_blocks: u64, expected_blocks: u64) -> bool {
        self.total_received_blocks.fetch_add(received_blocks, Ordering::SeqCst);
        self.total_expected_blocks.fetch_add(expected_blocks, Ordering::SeqCst);

        let mut samples = self.sync_rate_samples.write().unwrap();

        samples.push_back((received_blocks, expected_blocks));

        // Remove old samples. Usually is a single op after the window is full per 10s:
        while samples.len() > SYNC_RATE_WINDOW_MAX_SIZE {
            let (old_received_blocks, old_expected_blocks) = samples.pop_front().unwrap();
            self.total_received_blocks.fetch_sub(old_received_blocks, Ordering::SeqCst);
            self.total_expected_blocks.fetch_sub(old_expected_blocks, Ordering::SeqCst);
        }

        samples.len() >= SYNC_RATE_WINDOW_MIN_THRESHOLD
    }
}

/// SyncRateRule
/// Allow mining even if the node is "not nearly synced" if the sync rate is below threshold
/// and the finality point is recent. This is to prevent the network from undermining and to allow
/// the network to automatically recover from any short-term mining halt.
///
/// Trigger: Sync rate is below threshold and finality point is recent
/// Recovery: Sync rate is back above threshold
impl MiningRule for SyncRateRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData) {
        let expected_blocks = (extra_data.elapsed_time.as_millis() as u64) / extra_data.target_time_per_block;
        let received_blocks = delta.body_counts.max(delta.header_counts);

        if !self.update_sync_rate_window(received_blocks, expected_blocks) {
            // Don't process the sync rule if the window doesn't have enough samples to filter out noise
            return;
        }

        let rate: f64 =
            (self.total_received_blocks.load(Ordering::SeqCst) as f64) / (self.total_expected_blocks.load(Ordering::SeqCst) as f64);

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
