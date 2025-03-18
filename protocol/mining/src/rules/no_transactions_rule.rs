use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{trace, warn};

use super::{mining_rule::MiningRule, ExtraData};

/// NoTransactionsRule
/// Attempt to recover from consistent BadMerkleRoot errors by mining blocks without
/// any transactions.
///
/// Trigger: BadMerkleRoot error count is higher than the number of successfully validated blocks
/// Recovery: Two cooldown periods have passed
pub struct NoTransactionsRule {
    pub is_enabled: Arc<AtomicBool>,
    pub cooldown: AtomicU8,
}

impl NoTransactionsRule {
    pub fn new(is_enabled: Arc<AtomicBool>) -> Self {
        Self { is_enabled, cooldown: AtomicU8::new(0) }
    }
}

impl MiningRule for NoTransactionsRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, _extra_data: &ExtraData) {
        let cooldown_count = self.cooldown.load(Ordering::SeqCst);

        if cooldown_count > 0 {
            // Recovering
            if delta.submit_block_success_count > 0 || self.cooldown.fetch_sub(1, Ordering::SeqCst) == 1 {
                // Recovery condition #1: Any submit block RPC call succeeded in this interval
                // Recovery condition #2: Cooldown period has passed (important for low hashrate miners whose successful blocks are few and far between)
                self.cooldown.store(0, Ordering::SeqCst);
                self.is_enabled.store(false, Ordering::SeqCst);
                warn!("NoTransactionsRule: recovered | Bad Merkle Root Count: {}", delta.submit_block_bad_merkle_root_count);
            }
        } else if delta.submit_block_bad_merkle_root_count > 0 && delta.submit_block_success_count == 0 {
            // Triggered state
            // When submit block BadMerkleRoot errors occurred and there were no successfully submitted blocks
            if let Ok(false) = self.is_enabled.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                warn!(
                    "NoTransactionsRule: triggered | Bad Merkle Root Count: {} | Successfully submitted blocks: {}",
                    delta.submit_block_bad_merkle_root_count, delta.submit_block_success_count
                );
                self.cooldown.store(2, Ordering::Relaxed);
            }
        } else {
            // Normal state
            trace!(
                "NoTransactionsRule: normal | Bad Merkle Root Count: {} | Successfully submitted blocks: {}",
                delta.submit_block_bad_merkle_root_count,
                delta.submit_block_success_count,
            );
        }
    }
}
