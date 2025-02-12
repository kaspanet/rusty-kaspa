use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{trace, warn};

use super::mining_rule::MiningRule;

const VIRTUAL_PROCESSING_TRIGGER_THRESHOLD: f64 = 500.0; // 500 milliseconds
const VIRTUAL_PROCESSING_RECOVERY_THRESHOLD: f64 = 100.0; // 100 milliseconds

pub struct BlueParentsOnlyRule {
    pub is_enabled: Arc<AtomicBool>,
}

impl BlueParentsOnlyRule {
    pub fn new(is_enabled: Arc<AtomicBool>) -> Self {
        Self { is_enabled }
    }
}

impl MiningRule for BlueParentsOnlyRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot) {
        let received_blocks = delta.body_counts.max(delta.header_counts) as f64;
        let virtual_processing_avg_time_per_block_ms =
            if received_blocks > 0.0 { (delta.virtual_processing_time as f64) / received_blocks } else { 0.0 };
        trace!("Avg virtual processing time: {:.2}ms", virtual_processing_avg_time_per_block_ms);

        if virtual_processing_avg_time_per_block_ms > VIRTUAL_PROCESSING_TRIGGER_THRESHOLD {
            if let Ok(true) = self.is_enabled.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) {
                warn!("Mining Rule: Blue Parents Only");
            }
        } else if virtual_processing_avg_time_per_block_ms < VIRTUAL_PROCESSING_RECOVERY_THRESHOLD {
            // TODO: Add duration for how long before allowing to recover
            // maybe a merge depth bound duration has passed
            if let Ok(true) = self.is_enabled.compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed) {
                warn!("Mining Rule: Blue Parents Only recovered");
            }
        }
    }
}
