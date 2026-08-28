use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

mod appendable_segment_tree_api;
mod appendable_segment_tree_impl;
pub use appendable_segment_tree_api::{AppendableSegmentTreeApi, Bucket, bucket_for_score};
pub use appendable_segment_tree_impl::AppendableSegmentTree;
pub mod manager;
pub mod protocol;
pub mod rank_search;
pub mod tie_breaking;
#[cfg(feature = "baseline-debugging")]
pub mod umc_baseline;
pub mod umc_cascade;
pub mod umc_cascade_persistence;
pub mod umc_voting;

/// UMC cascade voting performance counters.
pub struct DagknightCounters {
    /// Total UMC cascade voting calls
    total_calls: AtomicU64,
    /// Total voting blocks (blues + reds, excluding grays) across all cascade calls
    total_voting_blocks: AtomicU64,
    /// Total cascade flips across all runs
    total_cascade_flips: AtomicU64,
    /// Maximum flips in a single cascade run
    max_cascade_flips: AtomicU64,
    /// Number of UMC cascade calls that started from a persisted checkpoint
    checkpoint_from_checkpoint: AtomicU64,
    /// Number of UMC cascade calls that started from scratch
    checkpoint_from_scratch: AtomicU64,
    /// Total estimated effort saved across all checkpoint loads,
    /// representing estimated blue blocks skipped by checkpointing
    checkpoint_estimated_effort_saved: AtomicU64,
    /// Total estimated effort across all UMC calls (sum of virtual_gd.blue_score).
    /// Used as denominator for effort_saved percentage.
    checkpoint_estimated_effort_total: AtomicU64,
    /// Baseline true, cascade false
    #[cfg(feature = "baseline-debugging")]
    baseline_true_cascade_false: AtomicU64,
    /// Baseline false, cascade true
    #[cfg(feature = "baseline-debugging")]
    baseline_false_cascade_true: AtomicU64,
}

impl Default for DagknightCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl DagknightCounters {
    pub fn new() -> Self {
        Self {
            total_calls: AtomicU64::new(0),
            total_voting_blocks: AtomicU64::new(0),
            total_cascade_flips: AtomicU64::new(0),
            max_cascade_flips: AtomicU64::new(0),
            checkpoint_from_checkpoint: AtomicU64::new(0),
            checkpoint_from_scratch: AtomicU64::new(0),
            checkpoint_estimated_effort_saved: AtomicU64::new(0),
            checkpoint_estimated_effort_total: AtomicU64::new(0),
            #[cfg(feature = "baseline-debugging")]
            baseline_true_cascade_false: AtomicU64::new(0),
            #[cfg(feature = "baseline-debugging")]
            baseline_false_cascade_true: AtomicU64::new(0),
        }
    }

    /// Record cascade performance statistics from a single run.
    pub fn record_cascade_stats(&self, flips: u64, voting_blocks: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
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

    /// Record a directional disagreement between baseline (paper Algorithm 6) and cascade.
    #[cfg(feature = "baseline-debugging")]
    pub fn record_baseline_disagreement(&self, baseline_accepted: bool, cascade_accepted: bool) {
        match (baseline_accepted, cascade_accepted) {
            (true, false) => {
                self.baseline_true_cascade_false.fetch_add(1, Ordering::Relaxed);
            }
            (false, true) => {
                self.baseline_false_cascade_true.fetch_add(1, Ordering::Relaxed);
            }
            _ => {} // agreement — no-op
        }
    }

    /// Record checkpoint persistence statistics from a single run_cascade call.
    pub fn record_checkpoint_stats(&self, from_checkpoint: bool, estimated_effort_saved: u64, estimated_effort_total: u64) {
        if from_checkpoint {
            self.checkpoint_from_checkpoint.fetch_add(1, Ordering::Relaxed);
        } else {
            self.checkpoint_from_scratch.fetch_add(1, Ordering::Relaxed);
        }
        self.checkpoint_estimated_effort_saved.fetch_add(estimated_effort_saved, Ordering::Relaxed);
        self.checkpoint_estimated_effort_total.fetch_add(estimated_effort_total, Ordering::Relaxed);
    }

    /// Take a point-in-time snapshot of all counters.
    pub fn snapshot(&self) -> DagknightCountersSnapshot {
        DagknightCountersSnapshot {
            total_calls: self.total_calls.load(Ordering::Relaxed),
            total_voting_blocks: self.total_voting_blocks.load(Ordering::Relaxed),
            total_cascade_flips: self.total_cascade_flips.load(Ordering::Relaxed),
            max_cascade_flips: self.max_cascade_flips.load(Ordering::Relaxed),
            checkpoint_from_checkpoint: self.checkpoint_from_checkpoint.load(Ordering::Relaxed),
            checkpoint_from_scratch: self.checkpoint_from_scratch.load(Ordering::Relaxed),
            checkpoint_estimated_effort_saved: self.checkpoint_estimated_effort_saved.load(Ordering::Relaxed),
            checkpoint_estimated_effort_total: self.checkpoint_estimated_effort_total.load(Ordering::Relaxed),
            #[cfg(feature = "baseline-debugging")]
            baseline_true_cascade_false: self.baseline_true_cascade_false.load(Ordering::Relaxed),
            #[cfg(feature = "baseline-debugging")]
            baseline_false_cascade_true: self.baseline_false_cascade_true.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of cascade voting counters.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DagknightCountersSnapshot {
    pub total_calls: u64,
    pub total_voting_blocks: u64,
    pub total_cascade_flips: u64,
    pub max_cascade_flips: u64,
    /// Number of UMC cascade calls that started from a persisted checkpoint
    pub checkpoint_from_checkpoint: u64,
    /// Number of UMC cascade calls that started from scratch
    pub checkpoint_from_scratch: u64,
    /// Total estimated effort saved across all checkpoint loads
    pub checkpoint_estimated_effort_saved: u64,
    /// Total estimated effort across all UMC calls (sum of virtual_gd.blue_score)
    pub checkpoint_estimated_effort_total: u64,
    #[cfg(feature = "baseline-debugging")]
    pub baseline_true_cascade_false: u64,
    #[cfg(feature = "baseline-debugging")]
    pub baseline_false_cascade_true: u64,
}

impl DagknightCountersSnapshot {
    /// Average voting blocks per cascade call.
    pub fn avg_voting_blocks_per_call(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.total_voting_blocks as f64 / self.total_calls as f64 }
    }

    /// Average flips per cascade call.
    pub fn avg_flips_per_call(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.total_cascade_flips as f64 / self.total_calls as f64 }
    }

    /// Percentage of UMC calls that used a checkpoint.
    pub fn checkpoint_hit_rate(&self) -> f64 {
        let total = self.checkpoint_from_checkpoint + self.checkpoint_from_scratch;
        if total == 0 { 0.0 } else { self.checkpoint_from_checkpoint as f64 / total as f64 * 100.0 }
    }

    /// Percentage of blue work saved by checkpointing relative to total blue work in conflict zones.
    pub fn checkpoint_effort_saved(&self) -> f64 {
        if self.checkpoint_estimated_effort_total == 0 {
            0.0
        } else {
            self.checkpoint_estimated_effort_saved as f64 / self.checkpoint_estimated_effort_total as f64 * 100.0
        }
    }

    /// Percentage of calls where baseline accepted and cascade rejected.
    #[cfg(feature = "baseline-debugging")]
    pub fn baseline_true_cascade_false_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.baseline_true_cascade_false as f64 / self.total_calls as f64 * 100.0 }
    }

    /// Percentage of calls where baseline rejected and cascade accepted.
    #[cfg(feature = "baseline-debugging")]
    pub fn baseline_false_cascade_true_percentage(&self) -> f64 {
        if self.total_calls == 0 { 0.0 } else { self.baseline_false_cascade_true as f64 / self.total_calls as f64 * 100.0 }
    }
}

impl std::ops::Sub for &DagknightCountersSnapshot {
    type Output = DagknightCountersSnapshot;

    fn sub(self, rhs: &DagknightCountersSnapshot) -> Self::Output {
        DagknightCountersSnapshot {
            total_calls: self.total_calls.saturating_sub(rhs.total_calls),
            total_voting_blocks: self.total_voting_blocks.saturating_sub(rhs.total_voting_blocks),
            total_cascade_flips: self.total_cascade_flips.saturating_sub(rhs.total_cascade_flips),
            max_cascade_flips: 0, // max doesn't subtract meaningfully
            checkpoint_from_checkpoint: self.checkpoint_from_checkpoint.saturating_sub(rhs.checkpoint_from_checkpoint),
            checkpoint_from_scratch: self.checkpoint_from_scratch.saturating_sub(rhs.checkpoint_from_scratch),
            checkpoint_estimated_effort_saved: self
                .checkpoint_estimated_effort_saved
                .saturating_sub(rhs.checkpoint_estimated_effort_saved),
            checkpoint_estimated_effort_total: self
                .checkpoint_estimated_effort_total
                .saturating_sub(rhs.checkpoint_estimated_effort_total),
            #[cfg(feature = "baseline-debugging")]
            baseline_true_cascade_false: self.baseline_true_cascade_false.saturating_sub(rhs.baseline_true_cascade_false),
            #[cfg(feature = "baseline-debugging")]
            baseline_false_cascade_true: self.baseline_false_cascade_true.saturating_sub(rhs.baseline_false_cascade_true),
        }
    }
}
