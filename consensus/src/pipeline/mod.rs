pub mod header_processor;

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct ProcessingCounters {
    pub blocks_submitted: AtomicU64,
    pub header_counts: AtomicU64,
    pub dep_counts: AtomicU64,
    // pub max_pending_headers: AtomicU64,
    // pub avg_pending_headers: AtomicU64,
}

impl ProcessingCounters {
    pub fn snapshot(&self) -> ProcessingCountersSnapshot {
        ProcessingCountersSnapshot {
            blocks_submitted: self.blocks_submitted.load(Ordering::SeqCst),
            header_counts: self.header_counts.load(Ordering::SeqCst),
            dep_counts: self.dep_counts.load(Ordering::SeqCst),
            // max_pending_headers: self.max_pending_headers.load(Ordering::SeqCst),
            // avg_pending_headers: self.avg_pending_headers.load(Ordering::SeqCst),
        }
    }
}

pub struct ProcessingCountersSnapshot {
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    // pub max_pending_headers: u64,
    // pub avg_pending_headers: u64,
}
