//! Streaming import of pruning-point SMT lanes.
//!
//! Processes sorted lanes in chunks: parallel leaf hashing via rayon,
//! then feeds to [`StreamingSmtBuilder`] with [`DbSink`] for batched DB writes.

use kaspa_smt::SmtHasher;
mod db_sink;

use std::time::Instant;

use std::collections::BTreeMap;

use kaspa_consensus_core::api::ImportLane;
use kaspa_database::prelude::{BatchDbWriter, DB, StoreError};
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::streaming::{StreamError, StreamingSmtBuilder};
use log::info;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rayon::prelude::IndexedParallelIterator;
use rocksdb::WriteBatch;

use crate::BlockHash;
use crate::keys::ScoreIndexKind;
use crate::processor::SmtStores;

use db_sink::DbSink;

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

/// Streams pre-chunked lane batches into the tree builder.
///
/// `chunks` yields `Vec<ImportLane>` already sized by the upstream
/// wire-level chunker (see `SMT_CHUNK_SIZE` in `protocol/flows/src/ibd/streams.rs`).
/// Each incoming Vec is processed as one step — parallel leaf hashing, proof
/// verification, DB batching, and `builder.feed`. No internal re-batching or
/// accumulator.
///
/// `max_batch_entries` remains the RocksDB `WriteBatch` flush threshold for
/// lane/score-index writes; it is independent of the incoming chunk size.
pub fn streaming_import(
    db: &DB,
    stores: &SmtStores,
    blue_score: u64,
    block_hash: BlockHash,
    total_count: u64,
    lanes_root: Hash,
    chunks: impl Iterator<Item = Vec<ImportLane>>,
    max_batch_entries: usize,
) -> Result<StreamingImportResult, StreamError<StoreError>> {
    if total_count == 0 {
        return Ok(StreamingImportResult { root: SeqCommitActiveNode::empty_root(), lanes_imported: 0, nodes_written: 0 });
    }

    let sink = DbSink::new(db, stores, blue_score, block_hash, max_batch_entries);
    let mut builder = StreamingSmtBuilder::<SeqCommitActiveNode, _>::new(total_count, sink);
    let mut lane_batch = WriteBatch::default();
    let mut batch_count = 0usize;
    let mut batch_id = 0u32;
    let mut score_groups: BTreeMap<u64, Vec<Hash>> = BTreeMap::new();
    let mut lanes_imported = 0u64;
    let mut progress = ImportProgress::new(total_count);
    let mut leaf_hashes: Vec<(Hash, Hash)> = Vec::new();

    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }

        chunk
            .par_iter()
            .map(|lane: &ImportLane| {
                let leaf_hash =
                    smt_leaf_hash(&SmtLeafInput { lane_key: &lane.lane_key, lane_tip: &lane.lane_tip, blue_score: lane.blue_score });
                (lane.lane_key, leaf_hash)
            })
            .collect_into_vec(&mut leaf_hashes);

        // Verify proofs against the expected lanes_root.
        for (lane, &(lane_key, leaf_hash)) in chunk.iter().zip(leaf_hashes.iter()) {
            let Some(proof) = &lane.proof else { continue };
            let Ok(true) = proof.verify::<SeqCommitActiveNode>(&lane_key, Some(leaf_hash), lanes_root) else {
                return Err(StreamError::ProofFailed(format!("lane {lane_key}")));
            };
        }

        write_lane_versions(stores, block_hash, &chunk, &mut lane_batch, &mut batch_count)?;
        write_score_index(stores, blue_score, block_hash, &chunk, &mut score_groups, &mut lane_batch, &mut batch_count, batch_id)?;

        if batch_count >= max_batch_entries {
            db.write(std::mem::take(&mut lane_batch)).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
            batch_count = 0;
        }
        batch_id += 1;

        for (lane_key, leaf_hash) in &leaf_hashes {
            builder.feed(*lane_key, *leaf_hash)?;
        }
        lanes_imported += chunk.len() as u64;
        progress.report(chunk.len());
    }

    progress.report_completion();

    let (root, mut sink) = builder.finish()?;
    sink.flush_batch().map_err(StreamError::Sink)?;
    flush_lane_batch(db, lane_batch, batch_count)?;

    Ok(StreamingImportResult { root, lanes_imported, nodes_written: sink.nodes_written() })
}

fn write_score_index(
    stores: &SmtStores,
    pp_blue_score: u64,
    block_hash: BlockHash,
    chunk: &[ImportLane],
    score_groups: &mut BTreeMap<u64, Vec<Hash>>,
    batch: &mut WriteBatch,
    batch_count: &mut usize,
    batch_id: u32,
) -> Result<(), StreamError<StoreError>> {
    // LeafUpdate: grouped by each lane's own blue_score
    score_groups.clear();
    for lane in chunk {
        score_groups.entry(lane.blue_score).or_default().push(lane.lane_key);
    }
    for (bs, keys) in score_groups.iter() {
        stores
            .score_index
            .put_batched(BatchDbWriter::new(batch), *bs, ScoreIndexKind::LeafUpdate, block_hash, keys, batch_id)
            .map_err(StreamError::Sink)?;
        *batch_count += 1;
    }

    // Structural: all lanes at the pruning point's blue_score (tree is built at this point)
    let all_keys: Vec<Hash> = chunk.iter().map(|l| l.lane_key).collect();
    stores
        .score_index
        .put_batched(BatchDbWriter::new(batch), pp_blue_score, ScoreIndexKind::Structural, block_hash, &all_keys, batch_id)
        .map_err(StreamError::Sink)?;
    *batch_count += 1;

    Ok(())
}

fn write_lane_versions(
    stores: &SmtStores,
    block_hash: BlockHash,
    chunk: &[ImportLane],
    lane_batch: &mut WriteBatch,
    lane_batch_count: &mut usize,
) -> Result<(), StreamError<StoreError>> {
    // Writes go directly to the DB lane-version store and intentionally skip
    // the in-memory lane cache. `SmtStores::get_lane` treats a cache hit as
    // authoritative (see the newest-suffix invariant in `crate::cache`), so
    // bypassing the cache is safe only because IBD SMT import runs after
    // `SmtStores::clear_all()` has emptied both the DB stores and the caches.
    // Thus there can be no stale cached lane versions disagreeing with the
    // imported DB state. After import the caches remain cold, and reads fall
    // back to DB until later incremental writes repopulate them.
    for lane in chunk {
        stores
            .lane_version
            .put(BatchDbWriter::new(lane_batch), lane.lane_key, lane.blue_score, block_hash, &lane.lane_tip)
            .map_err(StreamError::Sink)?;
    }
    // One RocksDB entry per lane — account for them as a single bump so the
    // flush threshold in `streaming_import` trips after roughly every
    // `max_batch_entries` lanes regardless of chunk size.
    *lane_batch_count += chunk.len();
    Ok(())
}

fn flush_lane_batch(db: &DB, lane_batch: WriteBatch, count: usize) -> Result<(), StreamError<StoreError>> {
    if count > 0 {
        db.write(lane_batch).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
    }
    Ok(())
}
