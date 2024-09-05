use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use mempool::tx::Priority;

mod block_template;
pub(crate) mod cache;
pub mod errors;
pub mod feerate;
pub mod manager;
mod manager_tests;
pub mod mempool;
pub mod model;
pub mod monitor;

// Exposed for benchmarks
pub use block_template::{policy::Policy, selector::RebalancingWeightedTransactionSelector};
pub use mempool::model::frontier::{feerate_key::FeerateTransactionKey, search_tree::SearchTree, Frontier};

#[cfg(test)]
pub mod testutils;

pub struct MiningCounters {
    pub creation_time: Instant,

    // Counters
    pub high_priority_tx_counts: AtomicU64,
    pub low_priority_tx_counts: AtomicU64,
    pub block_tx_counts: AtomicU64,
    pub tx_accepted_counts: AtomicU64,
    pub tx_evicted_counts: AtomicU64,
    pub input_counts: AtomicU64,
    pub output_counts: AtomicU64,

    // Samples
    pub ready_txs_sample: AtomicU64,
    pub txs_sample: AtomicU64,
    pub orphans_sample: AtomicU64,
    pub accepted_sample: AtomicU64,
}

impl Default for MiningCounters {
    fn default() -> Self {
        Self {
            creation_time: Instant::now(),
            high_priority_tx_counts: Default::default(),
            low_priority_tx_counts: Default::default(),
            block_tx_counts: Default::default(),
            tx_accepted_counts: Default::default(),
            tx_evicted_counts: Default::default(),
            input_counts: Default::default(),
            output_counts: Default::default(),
            ready_txs_sample: Default::default(),
            txs_sample: Default::default(),
            orphans_sample: Default::default(),
            accepted_sample: Default::default(),
        }
    }
}

impl MiningCounters {
    pub fn snapshot(&self) -> MempoolCountersSnapshot {
        MempoolCountersSnapshot {
            elapsed_time: (Instant::now() - self.creation_time),
            high_priority_tx_counts: self.high_priority_tx_counts.load(Ordering::Relaxed),
            low_priority_tx_counts: self.low_priority_tx_counts.load(Ordering::Relaxed),
            block_tx_counts: self.block_tx_counts.load(Ordering::Relaxed),
            tx_accepted_counts: self.tx_accepted_counts.load(Ordering::Relaxed),
            tx_evicted_counts: self.tx_evicted_counts.load(Ordering::Relaxed),
            input_counts: self.input_counts.load(Ordering::Relaxed),
            output_counts: self.output_counts.load(Ordering::Relaxed),
            ready_txs_sample: self.ready_txs_sample.load(Ordering::Relaxed),
            txs_sample: self.txs_sample.load(Ordering::Relaxed),
            orphans_sample: self.orphans_sample.load(Ordering::Relaxed),
            accepted_sample: self.accepted_sample.load(Ordering::Relaxed),
        }
    }

    pub fn p2p_tx_count_sample(&self) -> P2pTxCountSample {
        P2pTxCountSample {
            elapsed_time: (Instant::now() - self.creation_time),
            low_priority_tx_counts: self.low_priority_tx_counts.load(Ordering::Relaxed),
        }
    }

    pub fn increase_tx_counts(&self, value: u64, priority: Priority) {
        match priority {
            Priority::Low => {
                self.low_priority_tx_counts.fetch_add(value, Ordering::Relaxed);
            }
            Priority::High => {
                self.high_priority_tx_counts.fetch_add(value, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MempoolCountersSnapshot {
    pub elapsed_time: Duration,
    pub high_priority_tx_counts: u64,
    pub low_priority_tx_counts: u64,
    pub block_tx_counts: u64,
    pub tx_accepted_counts: u64,
    pub tx_evicted_counts: u64,
    pub input_counts: u64,
    pub output_counts: u64,
    pub ready_txs_sample: u64,
    pub txs_sample: u64,
    pub orphans_sample: u64,
    pub accepted_sample: u64,
}

impl MempoolCountersSnapshot {
    pub fn in_tx_counts(&self) -> u64 {
        self.high_priority_tx_counts + self.low_priority_tx_counts
    }

    /// Indicates whether this snapshot has any TPS activity which is worth logging
    pub fn has_tps_activity(&self) -> bool {
        self.tx_accepted_counts > 0 || self.block_tx_counts > 0 || self.low_priority_tx_counts > 0 || self.high_priority_tx_counts > 0
    }

    /// Returns an estimate of _Unique-TPS_, i.e. the number of unique transactions per second on average
    /// (excluding coinbase transactions)
    pub fn u_tps(&self) -> f64 {
        let elapsed = self.elapsed_time.as_secs_f64();
        if elapsed != 0f64 {
            self.tx_accepted_counts as f64 / elapsed
        } else {
            0f64
        }
    }

    /// Returns an estimate to the _Effective-TPS_ fraction which is a measure of how much of DAG capacity
    /// is utilized compared to the number of available mempool transactions. For instance a max
    /// value of `1.0` indicates that we cannot do any better in terms of throughput vs. current
    /// demand. A value close to `0.0` means that DAG capacity is mostly filled with duplicate
    /// transactions even though the mempool (demand) offers a much larger amount of unique transactions.   
    pub fn e_tps(&self) -> f64 {
        let accepted_txs = u64::min(self.ready_txs_sample, self.tx_accepted_counts); // The throughput
        let total_txs = u64::min(self.ready_txs_sample, self.block_tx_counts); // The min of demand and capacity
        if total_txs > 0 {
            accepted_txs as f64 / total_txs as f64
        } else {
            1f64 // No demand means we are 100% efficient
        }
    }
}

impl core::ops::Sub for &MempoolCountersSnapshot {
    type Output = MempoolCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            elapsed_time: self.elapsed_time.saturating_sub(rhs.elapsed_time),
            high_priority_tx_counts: self.high_priority_tx_counts.saturating_sub(rhs.high_priority_tx_counts),
            low_priority_tx_counts: self.low_priority_tx_counts.saturating_sub(rhs.low_priority_tx_counts),
            block_tx_counts: self.block_tx_counts.saturating_sub(rhs.block_tx_counts),
            tx_accepted_counts: self.tx_accepted_counts.saturating_sub(rhs.tx_accepted_counts),
            tx_evicted_counts: self.tx_evicted_counts.saturating_sub(rhs.tx_evicted_counts),
            input_counts: self.input_counts.saturating_sub(rhs.input_counts),
            output_counts: self.output_counts.saturating_sub(rhs.output_counts),
            ready_txs_sample: (self.ready_txs_sample + rhs.ready_txs_sample) / 2,
            txs_sample: (self.txs_sample + rhs.txs_sample) / 2,
            orphans_sample: (self.orphans_sample + rhs.orphans_sample) / 2,
            accepted_sample: (self.accepted_sample + rhs.accepted_sample) / 2,
        }
    }
}

/// Contains a snapshot of only the P2P transaction counter and time elapsed
pub struct P2pTxCountSample {
    pub elapsed_time: Duration,
    pub low_priority_tx_counts: u64,
}

impl core::ops::Sub for &P2pTxCountSample {
    type Output = P2pTxCountSample;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            elapsed_time: self.elapsed_time.saturating_sub(rhs.elapsed_time),
            low_priority_tx_counts: self.low_priority_tx_counts.saturating_sub(rhs.low_priority_tx_counts),
        }
    }
}
