use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use mempool::tx::Priority;

mod block_template;
pub(crate) mod cache;
pub mod errors;
pub mod manager;
mod manager_tests;
pub mod mempool;
pub mod model;
pub mod monitor;

#[cfg(test)]
pub mod testutils;

pub struct MiningCounters {
    pub creation_time: Instant,
    pub high_priority_tx_counts: AtomicU64,
    pub low_priority_tx_counts: AtomicU64,
    pub block_tx_counts: AtomicU64,
    pub tx_accepted_counts: AtomicU64,
    pub input_counts: AtomicU64,
    pub output_counts: AtomicU64,
    pub ready_txs_sample: AtomicU64,
}

impl Default for MiningCounters {
    fn default() -> Self {
        Self {
            creation_time: Instant::now(),
            high_priority_tx_counts: Default::default(),
            low_priority_tx_counts: Default::default(),
            block_tx_counts: Default::default(),
            tx_accepted_counts: Default::default(),
            input_counts: Default::default(),
            output_counts: Default::default(),
            ready_txs_sample: Default::default(),
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
            input_counts: self.input_counts.load(Ordering::Relaxed),
            output_counts: self.output_counts.load(Ordering::Relaxed),
            ready_txs_sample: self.ready_txs_sample.load(Ordering::Relaxed),
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
    pub input_counts: u64,
    pub output_counts: u64,
    pub ready_txs_sample: u64,
}

impl MempoolCountersSnapshot {
    pub fn in_tx_counts(&self) -> u64 {
        self.high_priority_tx_counts + self.low_priority_tx_counts
    }

    pub fn u_tps(&self) -> f64 {
        let elapsed = self.elapsed_time.as_secs_f64();
        if elapsed != 0f64 {
            self.tx_accepted_counts as f64 / elapsed
        } else {
            0f64
        }
    }

    pub fn e_tps(&self) -> f64 {
        let accepted_txs = u64::min(self.ready_txs_sample, self.tx_accepted_counts);
        let total_txs = u64::min(self.ready_txs_sample, self.block_tx_counts);
        if total_txs > 0 {
            accepted_txs as f64 / total_txs as f64
        } else {
            0f64
        }
    }
}

impl core::ops::Sub for &MempoolCountersSnapshot {
    type Output = MempoolCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            elapsed_time: self.elapsed_time.checked_sub(rhs.elapsed_time).unwrap_or_default(),
            high_priority_tx_counts: self.high_priority_tx_counts.checked_sub(rhs.high_priority_tx_counts).unwrap_or_default(),
            low_priority_tx_counts: self.low_priority_tx_counts.checked_sub(rhs.low_priority_tx_counts).unwrap_or_default(),
            block_tx_counts: self.block_tx_counts.checked_sub(rhs.block_tx_counts).unwrap_or_default(),
            tx_accepted_counts: self.tx_accepted_counts.checked_sub(rhs.tx_accepted_counts).unwrap_or_default(),
            input_counts: self.input_counts.checked_sub(rhs.input_counts).unwrap_or_default(),
            output_counts: self.output_counts.checked_sub(rhs.output_counts).unwrap_or_default(),
            ready_txs_sample: (self.ready_txs_sample + rhs.ready_txs_sample) / 2,
        }
    }
}
