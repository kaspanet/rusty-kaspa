use std::sync::{atomic::AtomicBool, Arc};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;

use super::mining_rule::MiningRule;

pub struct NoTransactionsRule {
    pub is_enabled: Arc<AtomicBool>,
}

impl NoTransactionsRule {
    pub fn new(is_enabled: Arc<AtomicBool>) -> Self {
        Self { is_enabled }
    }
}

impl MiningRule for NoTransactionsRule {
    fn check_rule(&self, _delta: &ProcessingCountersSnapshot) {
        // TODO: Add the rule and recovery condition
    }
}
