use std::{
    mem,
    time::{Duration, Instant},
};

use rocksdb::WriteBatch;

use kaspa_core::info;
use kaspa_database::prelude::DB;

#[derive(Default)]
struct CommitStats {
    commits: usize,
    total_ops: usize,
    max_ops: usize,
    total_bytes: usize,
    max_bytes: usize,
    total_duration: Duration,
    max_duration: Duration,
}

impl CommitStats {
    fn record(&mut self, ops: usize, bytes: usize, duration: Duration) {
        self.commits += 1;
        self.total_ops += ops;
        self.max_ops = self.max_ops.max(ops);
        self.total_bytes += bytes;
        self.max_bytes = self.max_bytes.max(bytes);
        self.total_duration += duration;
        self.max_duration = self.max_duration.max(duration);
    }
}

pub(super) struct PruningPhaseMetrics {
    started: Instant,
    batch_commit: CommitStats,
    total_traversed: usize,
    total_pruned: usize,
}

impl PruningPhaseMetrics {
    pub(super) fn new() -> Self {
        Self { started: Instant::now(), batch_commit: CommitStats::default(), total_traversed: 0, total_pruned: 0 }
    }

    pub(super) fn record_batch_commit(&mut self, ops: usize, bytes: usize, duration: Duration) {
        self.batch_commit.record(ops, bytes, duration);
    }

    pub(super) fn set_traversed(&mut self, traversed: usize, pruned: usize) {
        self.total_traversed = traversed;
        self.total_pruned = pruned;
    }

    pub(super) fn log_summary(&self) {
        let elapsed_ms = self.started.elapsed().as_millis();
        info!(
            "[PRUNING METRICS] config_batch_max_ms={} config_batch_max_ops={} config_batch_max_bytes={} duration_ms={} traversed={} pruned={}",
            PRUNE_BATCH_MAX_DURATION_MS,
            PRUNE_BATCH_MAX_OPS,
            PRUNE_BATCH_MAX_BYTES,
            elapsed_ms,
            self.total_traversed,
            self.total_pruned
        );
        if self.batch_commit.commits == 0 {
            return;
        }
        let avg_ops = self.batch_commit.total_ops as f64 / self.batch_commit.commits as f64;
        let avg_bytes = self.batch_commit.total_bytes as f64 / self.batch_commit.commits as f64;
        let avg_duration_ms = self.batch_commit.total_duration.as_secs_f64() * 1000.0 / self.batch_commit.commits as f64;
        info!(
            "[PRUNING METRICS] commit_type=batched count={} avg_ops={:.2} max_ops={} avg_bytes={:.2} max_bytes={} avg_commit_ms={:.3} max_commit_ms={:.3}",
            self.batch_commit.commits,
            avg_ops,
            self.batch_commit.max_ops,
            avg_bytes,
            self.batch_commit.max_bytes,
            avg_duration_ms,
            self.batch_commit.max_duration.as_secs_f64() * 1000.0
        );
    }
}

pub(super) const PRUNE_BATCH_MAX_BLOCKS: usize = 256;
pub(super) const PRUNE_BATCH_MAX_OPS: usize = 50_000;
pub(super) const PRUNE_BATCH_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(super) const PRUNE_BATCH_MAX_DURATION_MS: u64 = 50;
pub(super) const PRUNE_LOCK_TARGET_MAX_DURATION_MS: u64 = 5;

pub(super) struct PruneBatch {
    pub(super) batch: WriteBatch,
    block_count: usize,
    started: Option<Instant>,
}

impl PruneBatch {
    pub(super) fn new() -> Self {
        Self { batch: WriteBatch::default(), block_count: 0, started: None }
    }

    pub(super) fn on_block_staged(&mut self) {
        if self.block_count == 0 {
            self.started = Some(Instant::now());
        }
        self.block_count += 1;
    }

    pub(super) fn len(&self) -> usize {
        self.batch.len()
    }

    pub(super) fn size_in_bytes(&self) -> usize {
        self.batch.size_in_bytes()
    }

    pub(super) fn blocks(&self) -> usize {
        self.block_count
    }

    pub(super) fn elapsed(&self) -> Duration {
        self.started.map(|t| t.elapsed()).unwrap_or_default()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.batch.len() == 0
    }

    pub(super) fn take(&mut self) -> WriteBatch {
        self.block_count = 0;
        self.started = None;
        mem::take(&mut self.batch)
    }

    pub(super) fn should_flush(&self) -> bool {
        if self.is_empty() {
            return false;
        }

        self.blocks() >= PRUNE_BATCH_MAX_BLOCKS
            || self.len() >= PRUNE_BATCH_MAX_OPS
            || self.size_in_bytes() >= PRUNE_BATCH_MAX_BYTES
            || self.elapsed() >= Duration::from_millis(PRUNE_BATCH_MAX_DURATION_MS)
    }

    pub(super) fn flush(&mut self, db: &DB, metrics: &mut PruningPhaseMetrics) {
        if self.is_empty() {
            return;
        }

        let ops = self.len();
        let bytes = self.size_in_bytes();
        let commit_start = Instant::now();
        let write_batch = self.take();
        db.write(write_batch).unwrap();
        metrics.record_batch_commit(ops, bytes, commit_start.elapsed());
    }
}
