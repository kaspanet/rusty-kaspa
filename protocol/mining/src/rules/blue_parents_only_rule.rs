use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{trace, warn};

use super::{mining_rule::MiningRule, ExtraData};

const VIRTUAL_PROCESSING_TRIGGER_THRESHOLD: f64 = 500.0; // 500 milliseconds
const VIRTUAL_PROCESSING_RECOVERY_THRESHOLD: f64 = 100.0; // 100 milliseconds

/// BlueParentsOnlyRule
/// Attempt to recover from high virtual processing times possibly caused merging red blocks.
/// by only pointing to blue parents.
///
/// Trigger: virtual processing average time is above threshold
/// Recovery: virtual processing average time is below threshold and a merge depth bound has passed
pub struct BlueParentsOnlyRule {
    pub is_enabled: Arc<AtomicBool>,
    pub trigger_daa_score: AtomicU64,
}

impl BlueParentsOnlyRule {
    pub fn new(is_enabled: Arc<AtomicBool>) -> Self {
        Self { is_enabled, trigger_daa_score: AtomicU64::new(0) }
    }
}

impl MiningRule for BlueParentsOnlyRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData) {
        let received_blocks = delta.body_counts.max(delta.header_counts) as f64;
        let sink_daa_score = extra_data.sink_daa_score_timestamp.daa_score;
        // DAA score may not be monotonic, so use saturating_sub
        let score_since_trigger = sink_daa_score.saturating_sub(self.trigger_daa_score.load(Ordering::Relaxed));
        let virtual_processing_avg_time_per_block_ms =
            if received_blocks > 0.0 { (delta.virtual_processing_time as f64) / received_blocks } else { 0.0 };

        if self.is_enabled.load(Ordering::Relaxed) {
            // Rule is triggered. Check for recovery
            if virtual_processing_avg_time_per_block_ms < VIRTUAL_PROCESSING_RECOVERY_THRESHOLD
                && score_since_trigger >= extra_data.merge_depth
            {
                self.is_enabled.store(false, Ordering::SeqCst);
                warn!(
                    "BlueParentsOnlyRule: recovered | Avg virtual processing time: {:.2}ms",
                    virtual_processing_avg_time_per_block_ms,
                );
            } else {
                trace!(
                    "BlueParentsOnlyRule: active | Avg virtual processing time: {:.2}ms | Score since trigger: {}",
                    virtual_processing_avg_time_per_block_ms,
                    score_since_trigger
                );
            }
        } else {
            // Rule is not triggered. Check for trigger
            if virtual_processing_avg_time_per_block_ms > VIRTUAL_PROCESSING_TRIGGER_THRESHOLD {
                self.is_enabled.store(true, Ordering::SeqCst);
                self.trigger_daa_score.store(sink_daa_score, Ordering::Relaxed);
                warn!(
                    "BlueParentsOnlyRule: triggered | Avg virtual processing time: {:.2}ms",
                    virtual_processing_avg_time_per_block_ms
                );
            } else {
                trace!("BlueParentsOnlyRule: normal | Avg virtual processing time: {:.2}ms", virtual_processing_avg_time_per_block_ms);
            }
        }
    }
}
