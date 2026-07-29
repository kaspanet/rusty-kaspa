use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

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

/// UMC cascade voting counters: agreement validation and cascade performance stats.
pub struct DagknightCounters {
    /// Calls where original and proposed votes were identical
    identical: AtomicU64,
    /// Calls where original and proposed votes differed
    differences: AtomicU64,
    /// Calls where original was true and proposed was false
    original_true_proposed_false: AtomicU64,
    /// Calls where original was false and proposed was true
    original_false_proposed_true: AtomicU64,
    /// Total UMC cascade voting calls
    total_calls: AtomicU64,
    /// Baseline true, proposed false
    baseline_true_proposed_false: AtomicU64,
    /// Baseline false, proposed true
    baseline_false_proposed_true: AtomicU64,
    /// Total voting blocks (blues + reds, excluding grays) across all cascade calls
    total_voting_blocks: AtomicU64,
    /// Total cascade flips across all runs
    total_cascade_flips: AtomicU64,
    /// Maximum flips in a single cascade run
    max_cascade_flips: AtomicU64,
}

impl Default for DagknightCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl DagknightCounters {
    pub fn new() -> Self {
        Self {
            identical: AtomicU64::new(0),
            differences: AtomicU64::new(0),
            original_true_proposed_false: AtomicU64::new(0),
            original_false_proposed_true: AtomicU64::new(0),
            total_calls: AtomicU64::new(0),
            baseline_true_proposed_false: AtomicU64::new(0),
            baseline_false_proposed_true: AtomicU64::new(0),
            total_voting_blocks: AtomicU64::new(0),
            total_cascade_flips: AtomicU64::new(0),
            max_cascade_flips: AtomicU64::new(0),
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

    /// Record cascade performance statistics from a single run.
    pub fn record_cascade_stats(&self, flips: u64, voting_blocks: u64) {
        self.total_cascade_flips.fetch_add(flips, Ordering::Relaxed);
        self.total_voting_blocks.fetch_add(voting_blocks, Ordering::Relaxed);
        let mut current = self.max_cascade_flips.load(Ordering::Relaxed);
        while flips > current {
            match self.max_cascade_flips.compare_exchange_weak(current, flips, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Record a directional disagreement between baseline (paper Algorithm 6) and proposed cascade.
    pub fn record_baseline_disagreement(&self, baseline_true: bool, proposed_true: bool) {
        match (baseline_true, proposed_true) {
            (true, false) => {
                self.baseline_true_proposed_false.fetch_add(1, Ordering::Relaxed);
            }
            (false, true) => {
                self.baseline_false_proposed_true.fetch_add(1, Ordering::Relaxed);
            }
            _ => {} // agreement — no-op
        }
    }

    /// Take a point-in-time snapshot of all counters.
    pub fn snapshot(&self) -> DagknightCountersSnapshot {
        DagknightCountersSnapshot {
            total_calls: self.total_calls.load(Ordering::Relaxed),
            identical: self.identical.load(Ordering::Relaxed),
            differences: self.differences.load(Ordering::Relaxed),
            original_true_proposed_false: self.original_true_proposed_false.load(Ordering::Relaxed),
            original_false_proposed_true: self.original_false_proposed_true.load(Ordering::Relaxed),
            baseline_true_proposed_false: self.baseline_true_proposed_false.load(Ordering::Relaxed),
            baseline_false_proposed_true: self.baseline_false_proposed_true.load(Ordering::Relaxed),
            total_voting_blocks: self.total_voting_blocks.load(Ordering::Relaxed),
            total_cascade_flips: self.total_cascade_flips.load(Ordering::Relaxed),
            max_cascade_flips: self.max_cascade_flips.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of cascade voting counters.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DagknightCountersSnapshot {
    pub total_calls: u64,
    pub identical: u64,
    pub differences: u64,
    pub original_true_proposed_false: u64,
    pub original_false_proposed_true: u64,
    pub baseline_true_proposed_false: u64,
    pub baseline_false_proposed_true: u64,
    pub total_voting_blocks: u64,
    pub total_cascade_flips: u64,
    pub max_cascade_flips: u64,
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

    /// Percentage of calls where baseline accepted and proposed rejected.
    pub fn baseline_true_proposed_false_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.baseline_true_proposed_false as f64 / self.total_calls as f64 * 100.0 }
    }

    /// Percentage of calls where baseline rejected and proposed accepted.
    pub fn baseline_false_proposed_true_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.baseline_false_proposed_true as f64 / self.total_calls as f64 * 100.0 }
    }

    /// Average voting blocks per cascade call.
    pub fn avg_voting_blocks_per_call(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.total_voting_blocks as f64 / self.total_calls as f64 }
    }

    /// Average flips per cascade call.
    pub fn avg_flips_per_call(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.total_cascade_flips as f64 / self.total_calls as f64 }
    }
}

impl std::ops::Sub for &DagknightCountersSnapshot {
    type Output = DagknightCountersSnapshot;

    fn sub(self, rhs: &DagknightCountersSnapshot) -> Self::Output {
        DagknightCountersSnapshot {
            total_calls: self.total_calls.saturating_sub(rhs.total_calls),
            identical: self.identical.saturating_sub(rhs.identical),
            differences: self.differences.saturating_sub(rhs.differences),
            original_true_proposed_false: self.original_true_proposed_false.saturating_sub(rhs.original_true_proposed_false),
            original_false_proposed_true: self.original_false_proposed_true.saturating_sub(rhs.original_false_proposed_true),
            baseline_true_proposed_false: self.baseline_true_proposed_false.saturating_sub(rhs.baseline_true_proposed_false),
            baseline_false_proposed_true: self.baseline_false_proposed_true.saturating_sub(rhs.baseline_false_proposed_true),
            total_voting_blocks: self.total_voting_blocks.saturating_sub(rhs.total_voting_blocks),
            total_cascade_flips: self.total_cascade_flips.saturating_sub(rhs.total_cascade_flips),
            max_cascade_flips: 0, // max doesn't subtract meaningfully
        }
    }
}
