//! Streaming import of pruning-point SMT lanes.
//!
//! Processes sorted lanes in chunks: parallel leaf hashing via rayon,
//! then feeds to [`StreamingSmtBuilder`] with [`DbSink`] for batched DB writes.

use kaspa_smt::SmtHasher;
mod db_sink;

use std::time::Instant;

use kaspa_database::prelude::{BatchDbWriter, DB, StoreError};
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::streaming::{StreamError, StreamingSmtBuilder};
use log::info;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rayon::prelude::IndexedParallelIterator;
use rocksdb::WriteBatch;

use crate::processor::SmtStores;
use crate::{BlockHash, LaneKey};

use db_sink::DbSink;

pub struct StreamingImportLane {
    pub lane_key: LaneKey,
    pub lane_tip: Hash,
    pub blue_score: u64,
}

pub struct StreamingImportResult {
    pub root: Hash,
    pub lanes_imported: u64,
    pub nodes_written: usize,
}

struct ImportProgress {
    total_lanes: u64,
    lanes_processed: u64,
    last_log_time: Instant,
}

impl ImportProgress {
    fn new(total_lanes: u64) -> Self {
        Self { total_lanes, lanes_processed: 0, last_log_time: Instant::now() }
    }

    fn report(&mut self, delta: usize) {
        self.lanes_processed += delta as u64;
        let now = Instant::now();
        if now.duration_since(self.last_log_time) >= std::time::Duration::from_secs(2) {
            let pct = (self.lanes_processed as f64 / self.total_lanes as f64 * 100.0) as u32;
            info!("SMT import {} of {} ({}%)", self.lanes_processed, self.total_lanes, pct);
            self.last_log_time = now;
        }
    }

    fn report_completion(&self) {
        info!("SMT import complete ({} lanes)", self.lanes_processed);
    }
}

pub fn streaming_import(
    db: &DB,
    stores: &SmtStores,
    blue_score: u64,
    block_hash: BlockHash,
    total_count: u64,
    lanes: impl Iterator<Item = StreamingImportLane>,
    max_batch_entries: usize,
) -> Result<StreamingImportResult, StreamError<StoreError>> {
    if total_count == 0 {
        return Ok(StreamingImportResult { root: SeqCommitActiveNode::empty_root(), lanes_imported: 0, nodes_written: 0 });
    }

    let sink = DbSink::new(db, stores, blue_score, block_hash, max_batch_entries);
    let mut builder = StreamingSmtBuilder::<SeqCommitActiveNode, _>::new(total_count, sink);
    let mut lane_batch = WriteBatch::default();
    let mut lane_batch_count = 0usize;
    let mut lanes_imported = 0u64;
    let mut progress = ImportProgress::new(total_count);

    let mut chunk = Vec::with_capacity(max_batch_entries);
    let mut leaf_hashes = Vec::with_capacity(max_batch_entries);

    let mut step = |chunk: &mut Vec<_>, leaf_hashes: &mut Vec<_>| -> Result<(), StreamError<StoreError>> {
        chunk
            .par_iter()
            .map(|lane: &StreamingImportLane| {
                let leaf_hash =
                    smt_leaf_hash(&SmtLeafInput { lane_key: &lane.lane_key, lane_tip: &lane.lane_tip, blue_score: lane.blue_score });
                (lane.lane_key, leaf_hash)
            })
            .collect_into_vec(leaf_hashes);

        write_lane_versions(db, stores, block_hash, chunk, &mut lane_batch, &mut lane_batch_count, max_batch_entries)?;

        for (lane_key, leaf_hash) in leaf_hashes {
            builder.feed(*lane_key, *leaf_hash)?;
        }
        lanes_imported += chunk.len() as u64;
        progress.report(chunk.len());
        chunk.clear();
        Ok(())
    };
    for lane in lanes {
        chunk.push(lane);
        if chunk.len() < max_batch_entries {
            continue;
        }
        step(&mut chunk, &mut leaf_hashes)?;
    }
    step(&mut chunk, &mut leaf_hashes)?;

    progress.report_completion();

    let (root, mut sink) = builder.finish()?;
    sink.flush_batch().map_err(StreamError::Sink)?;
    flush_lane_batch(db, lane_batch, lane_batch_count)?;

    // TODO(KIP-21): re-enable score_index writes once batch optimization is implemented

    Ok(StreamingImportResult { root, lanes_imported, nodes_written: sink.nodes_written() })
}

fn write_lane_versions(
    db: &DB,
    stores: &SmtStores,
    block_hash: BlockHash,
    chunk: &[StreamingImportLane],
    lane_batch: &mut WriteBatch,
    lane_batch_count: &mut usize,
    max_batch_entries: usize,
) -> Result<(), StreamError<StoreError>> {
    for lane in chunk {
        stores
            .lane_version
            .put(BatchDbWriter::new(lane_batch), lane.lane_key, lane.blue_score, block_hash, &lane.lane_tip)
            .map_err(StreamError::Sink)?;
        *lane_batch_count += 1;
        if *lane_batch_count >= max_batch_entries {
            db.write(std::mem::take(lane_batch)).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
            *lane_batch_count = 0;
        }
    }
    Ok(())
}

fn flush_lane_batch(db: &DB, lane_batch: WriteBatch, count: usize) -> Result<(), StreamError<StoreError>> {
    if count > 0 {
        db.write(lane_batch).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
    }
    Ok(())
}
