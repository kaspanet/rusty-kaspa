use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc,
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{trace, warn};

use super::{mining_rule::MiningRule, ExtraData};

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
        let cooldown_count = self.cooldown.load(Ordering::Relaxed);

        if cooldown_count > 0 {
            // Recovering
            // BadMerkleRoot cannot occur in blocks without transactions, so we have to recover from this state
            // through some other way
            if self.cooldown.fetch_sub(1, Ordering::Relaxed) == 1 {
                // Recovered state
                if let Ok(true) = self.is_enabled.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst) {
                    warn!("NoTransactionsRule: recovered | Bad Merkle Root Count: {}", delta.bad_merkle_root_count);
                } else {
                    trace!(
                        "NoTransactionsRule: recovering | Bad Merkle Root Count: {} | Valid Body Count: {} | Cooldown: {}",
                        delta.bad_merkle_root_count,
                        delta.body_counts,
                        cooldown_count
                    );
                }
            }
        } else if delta.bad_merkle_root_count > delta.body_counts {
            // Triggered state
            // This occurs when during this interval there were more blocks that resulted in bad merkle root errors than successfully validated blocks
            if let Ok(false) = self.is_enabled.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                warn!(
                    "NoTransactionsRule: triggered | Bad Merkle Root Count: {} | Valid Body Count: {}",
                    delta.bad_merkle_root_count, delta.body_counts
                );
                self.cooldown.store(2, Ordering::Relaxed);
            }
        } else {
            // Normal state
            trace!(
                "NoTransactionsRule: normal | Bad Merkle Root Count: {} | Valid Body Count: {}",
                delta.bad_merkle_root_count,
                delta.body_counts
            );
        }
    }
}
