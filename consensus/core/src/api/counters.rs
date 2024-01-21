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
            blocks_submitted: self.blocks_submitted.saturating_sub(rhs.blocks_submitted),
            header_counts: self.header_counts.saturating_sub(rhs.header_counts),
            dep_counts: self.dep_counts.saturating_sub(rhs.dep_counts),
            mergeset_counts: self.mergeset_counts.saturating_sub(rhs.mergeset_counts),
            body_counts: self.body_counts.saturating_sub(rhs.body_counts),
            txs_counts: self.txs_counts.saturating_sub(rhs.txs_counts),
            chain_block_counts: self.chain_block_counts.saturating_sub(rhs.chain_block_counts),
            mass_counts: self.mass_counts.saturating_sub(rhs.mass_counts),
        }
    }
}
