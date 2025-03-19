use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{trace, warn};

use super::{mining_rule::MiningRule, ExtraData};

/// BlueParentsOnlyRule
/// Attempt to recover from high build block template times (possibly caused by merging red blocks)
/// by disallowing reds in the mergeset.
///
/// Trigger: build block template call durations above threshold were observed and there were no calls
///          that were below threshold
/// Recovery: build block template call durations within threshold observed and
///           a merge depth bound period has passed
pub struct BlueParentsOnlyRule {
    pub is_enabled: Arc<AtomicBool>,
    pub trigger_daa_score: AtomicU64,
    pub within_threshold_calls_after_trigger: AtomicU64,
    pub above_threshold_calls_after_trigger: AtomicU64,
}

impl BlueParentsOnlyRule {
    pub fn new(is_enabled: Arc<AtomicBool>) -> Self {
        Self {
            is_enabled,
            trigger_daa_score: AtomicU64::new(0),
            within_threshold_calls_after_trigger: AtomicU64::new(0),
            above_threshold_calls_after_trigger: AtomicU64::new(0),
        }
    }
}

impl MiningRule for BlueParentsOnlyRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData) {
        let sink_daa_score = extra_data.sink_daa_score_timestamp.daa_score;
        // DAA score may not be monotonic, so use saturating_sub
        let score_since_trigger = sink_daa_score.saturating_sub(self.trigger_daa_score.load(Ordering::Relaxed));

        if self.is_enabled.load(Ordering::SeqCst) {
            // Rule is triggered. Check for recovery
            let within_threshold_calls =
                self.within_threshold_calls_after_trigger.fetch_add(delta.build_block_template_within_threshold, Ordering::SeqCst)
                    + delta.build_block_template_within_threshold;
            let above_threshold_calls =
                self.above_threshold_calls_after_trigger.fetch_add(delta.build_block_template_above_threshold, Ordering::SeqCst)
                    + delta.build_block_template_above_threshold;

            if score_since_trigger >= extra_data.merge_depth && within_threshold_calls > 0 {
                // Recovery condition met: A merge depth bound has passed and calls within threshold were observed
                self.is_enabled.store(false, Ordering::SeqCst);
                self.within_threshold_calls_after_trigger.store(0, Ordering::SeqCst);
                self.above_threshold_calls_after_trigger.store(0, Ordering::SeqCst);
                warn!("BlueParentsOnlyRule: recovered  | No. of Block Template Build Times within/above threshold since trigger: {}/{} | Score since trigger: {}",
                    within_threshold_calls, above_threshold_calls, score_since_trigger);
            } else {
                warn!(
                    "BlueParentsOnlyRule: active | No. of Block Template Build Times within/above threshold since trigger: {}/{} | Score since trigger: {}",
                    within_threshold_calls, above_threshold_calls, score_since_trigger
                );
            }
        } else {
            // Rule is not triggered. Check for trigger
            if delta.build_block_template_within_threshold == 0 && delta.build_block_template_above_threshold > 0 {
                self.is_enabled.store(true, Ordering::SeqCst);
                self.trigger_daa_score.store(sink_daa_score, Ordering::SeqCst);
                warn!(
                    "BlueParentsOnlyRule: triggered | No. of Block Template Build Times within/above threshold: {}/{}",
                    delta.build_block_template_within_threshold, delta.build_block_template_above_threshold
                );
            } else {
                trace!(
                    "BlueParentsOnlyRule: normal | No. of Block Template Build Times within/above threshold: {}/{}",
                    delta.build_block_template_within_threshold,
                    delta.build_block_template_above_threshold
                );
            }
        }
    }
}
