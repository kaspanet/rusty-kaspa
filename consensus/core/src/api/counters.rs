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
    pub chain_disqualified_counts: AtomicU64,
    pub storage_mass_counts: AtomicU64,
    pub compute_mass_counts: AtomicU64,
    pub transient_mass_counts: AtomicU64,
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
            chain_disqualified_counts: self.chain_disqualified_counts.load(Ordering::Relaxed),
            storage_mass_counts: self.storage_mass_counts.load(Ordering::Relaxed),
            compute_mass_counts: self.compute_mass_counts.load(Ordering::Relaxed),
            transient_mass_counts: self.transient_mass_counts.load(Ordering::Relaxed),
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct ProcessingCountersSnapshot {
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    pub mergeset_counts: u64,
    pub body_counts: u64,
    pub txs_counts: u64,
    pub chain_block_counts: u64,
    pub chain_disqualified_counts: u64,
    pub storage_mass_counts: u64,
    pub compute_mass_counts: u64,
    pub transient_mass_counts: u64,
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
            chain_disqualified_counts: self.chain_disqualified_counts.saturating_sub(rhs.chain_disqualified_counts),
            storage_mass_counts: self.storage_mass_counts.saturating_sub(rhs.storage_mass_counts),
            compute_mass_counts: self.compute_mass_counts.saturating_sub(rhs.compute_mass_counts),
            transient_mass_counts: self.transient_mass_counts.saturating_sub(rhs.transient_mass_counts),
        }
    }
}
