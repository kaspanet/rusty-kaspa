pub mod body_processor;
pub mod deps_manager;
pub mod header_processor;
pub mod monitor;
pub mod pruning_processor;
pub mod virtual_processor;

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct ProcessingCounters {
    pub blocks_submitted: AtomicU64,
    pub header_counts: AtomicU64,
    pub dep_counts: AtomicU64,
    pub mergeset_counts: AtomicU64,
    pub body_counts: AtomicU64,
    pub txs_counts: AtomicU64,
    pub chain_block_counts: AtomicU64,
    pub mass_counts: AtomicU64,
}

impl ProcessingCounters {
    pub fn snapshot(&self) -> ProcessingCountersSnapshot {
        ProcessingCountersSnapshot {
            blocks_submitted: self.blocks_submitted.load(Ordering::Relaxed),
            header_counts: self.header_counts.load(Ordering::Relaxed),
            dep_counts: self.dep_counts.load(Ordering::Relaxed),
            mergeset_counts: self.mergeset_counts.load(Ordering::Relaxed),
            body_counts: self.body_counts.load(Ordering::Relaxed),
            txs_counts: self.txs_counts.load(Ordering::Relaxed),
            chain_block_counts: self.chain_block_counts.load(Ordering::Relaxed),
            mass_counts: self.mass_counts.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProcessingCountersSnapshot {
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    pub mergeset_counts: u64,
    pub body_counts: u64,
    pub txs_counts: u64,
    pub chain_block_counts: u64,
    pub mass_counts: u64,
}

impl core::ops::Sub for &ProcessingCountersSnapshot {
    type Output = ProcessingCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            blocks_submitted: self.blocks_submitted.checked_sub(rhs.blocks_submitted).unwrap_or_default(),
            header_counts: self.header_counts.checked_sub(rhs.header_counts).unwrap_or_default(),
            dep_counts: self.dep_counts.checked_sub(rhs.dep_counts).unwrap_or_default(),
            mergeset_counts: self.mergeset_counts.checked_sub(rhs.mergeset_counts).unwrap_or_default(),
            body_counts: self.body_counts.checked_sub(rhs.body_counts).unwrap_or_default(),
            txs_counts: self.txs_counts.checked_sub(rhs.txs_counts).unwrap_or_default(),
            chain_block_counts: self.chain_block_counts.checked_sub(rhs.chain_block_counts).unwrap_or_default(),
            mass_counts: self.mass_counts.checked_sub(rhs.mass_counts).unwrap_or_default(),
        }
    }
}
