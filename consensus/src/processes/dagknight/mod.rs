use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use kaspa_consensus_core::KType;
use kaspa_hashes::Hash;

use crate::processes::ghostdag::ordering::SortableBlock;

mod appendable_segment_tree_api;
mod appendable_segment_tree_impl;
pub use appendable_segment_tree_api::{AppendableSegmentTreeApi, Bucket, bucket_for_score};
pub use appendable_segment_tree_impl::AppendableSegmentTree;
pub mod manager;
pub mod protocol;
pub mod rank_search;
pub mod tie_breaking;
pub mod umc_cascade;

pub struct GroupMetadata {
    conflict_genesis: Hash,
    subgroup: Arc<Vec<Hash>>,
    k: KType,
    selected_parent: SortableBlock,
}

/// Counters tracking UMC cascade voting statistics across original vs proposed implementations.
/// Uses atomic counters for lock-free concurrent access during DAG processing.
pub struct DagknightCounters {
    creation_time: Instant,
    /// Total UMC cascade voting calls
    total_calls: AtomicU64,
    /// Calls where original and proposed votes were identical
    identical: AtomicU64,
    /// Calls where original and proposed votes differed
    differences: AtomicU64,
    /// Calls where original was true and proposed was false
    original_true_proposed_false: AtomicU64,
    /// Calls where original was false and proposed was true
    original_false_proposed_true: AtomicU64,
}

impl Default for DagknightCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl DagknightCounters {
    pub fn new() -> Self {
        Self {
            creation_time: Instant::now(),
            total_calls: AtomicU64::new(0),
            identical: AtomicU64::new(0),
            differences: AtomicU64::new(0),
            original_true_proposed_false: AtomicU64::new(0),
            original_false_proposed_true: AtomicU64::new(0),
        }
    }

    /// Record a UMC vote comparison result.
    /// TEMPORARY
    pub fn record_vote(&self, original: bool, proposed: bool) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        if original == proposed {
            self.identical.fetch_add(1, Ordering::Relaxed);
        } else {
            self.differences.fetch_add(1, Ordering::Relaxed);
            if original && !proposed {
                self.original_true_proposed_false.fetch_add(1, Ordering::Relaxed);
            } else {
                self.original_false_proposed_true.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Take a point-in-time snapshot of all counters.
    pub fn snapshot(&self) -> DagknightCountersSnapshot {
        DagknightCountersSnapshot {
            elapsed_time: Instant::now().duration_since(self.creation_time),
            total_calls: self.total_calls.load(Ordering::Relaxed),
            identical: self.identical.load(Ordering::Relaxed),
            differences: self.differences.load(Ordering::Relaxed),
            original_true_proposed_false: self.original_true_proposed_false.load(Ordering::Relaxed),
            original_false_proposed_true: self.original_false_proposed_true.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of DagknightCounters.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DagknightCountersSnapshot {
    pub elapsed_time: Duration,
    pub total_calls: u64,
    pub identical: u64,
    pub differences: u64,
    pub original_true_proposed_false: u64,
    pub original_false_proposed_true: u64,
}

impl DagknightCountersSnapshot {
    /// Percentage of calls where the original and proposed votes differed.
    pub fn difference_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.differences as f64 / self.total_calls as f64 * 100.0 }
    }

    /// Percentage of calls where original was true and proposed was false.
    pub fn original_true_proposed_false_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.original_true_proposed_false as f64 / self.total_calls as f64 * 100.0 }
    }

    /// Percentage of calls where original was false and proposed was true.
    pub fn original_false_proposed_true_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.original_false_proposed_true as f64 / self.total_calls as f64 * 100.0 }
    }
}

impl std::ops::Sub for &DagknightCountersSnapshot {
    type Output = DagknightCountersSnapshot;

    fn sub(self, rhs: &DagknightCountersSnapshot) -> Self::Output {
        DagknightCountersSnapshot {
            elapsed_time: self.elapsed_time.saturating_sub(rhs.elapsed_time),
            total_calls: self.total_calls.saturating_sub(rhs.total_calls),
            identical: self.identical.saturating_sub(rhs.identical),
            differences: self.differences.saturating_sub(rhs.differences),
            original_true_proposed_false: self.original_true_proposed_false.saturating_sub(rhs.original_true_proposed_false),
            original_false_proposed_true: self.original_false_proposed_true.saturating_sub(rhs.original_false_proposed_true),
        }
    }
}
