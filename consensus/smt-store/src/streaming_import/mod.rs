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
/// verification, lane-version DB batching, and `builder.feed`.
///
/// Score-index writes are emitted as soon as a chunk's lanes are all sealed
/// (per-lane seal events fed into [`DbSink`] via [`MergeSink::record_seal`]
/// callbacks). Pending entries that contain still-unsealed lanes wait until
/// later feeds or `builder.finish()` complete the sealing. After each flush
/// the corresponding lane entries are dropped from the per-lane map so its
/// footprint stays bounded by unflushed lanes.
///
/// `max_batch_entries` is the RocksDB `WriteBatch` flush threshold for
/// lane-version and score-index writes; it is independent of the incoming
/// chunk size.
pub fn streaming_import(
    db: &DB,
    stores: &SmtStores,
    block_hash: BlockHash,
    total_count: u64,
    lanes_root: Hash,
    chunks: impl Iterator<Item = Vec<ImportLane>>,
    max_batch_entries: usize,
) -> Result<StreamingImportResult, StreamError<StoreError>> {
    if total_count == 0 {
        return Ok(StreamingImportResult { root: SeqCommitActiveNode::empty_root(), lanes_imported: 0, nodes_written: 0 });
    }

    // `branch_version` writes are versioned per-leaf (see `DbSink::write_node`)
    // so they age out of the read window at the same rate the live processor
    // would have produced. The sink itself doesn't need a sink-wide bs.
    let sink = DbSink::new(db, stores, block_hash, max_batch_entries);
    let mut builder = StreamingSmtBuilder::<SeqCommitActiveNode, _>::new(total_count, sink);
    let mut lane_batch = WriteBatch::default();
    let mut batch_count = 0usize;
    let mut batch_id = 0u32;
    let mut pending: Vec<PendingScoreIndex> = Vec::new();
    let mut score_batch = WriteBatch::default();
    let mut score_batch_count = 0usize;
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
                let leaf_hash = smt_leaf_hash(&SmtLeafInput { lane_tip: &lane.lane_tip, blue_score: lane.blue_score });
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

        for (lane, &(lane_key, leaf_hash)) in chunk.iter().zip(leaf_hashes.iter()) {
            builder.feed(lane_key, leaf_hash, lane.blue_score)?;
        }

        write_lane_versions(stores, block_hash, &chunk, &mut lane_batch, &mut batch_count)?;
        pending.push(PendingScoreIndex::from_chunk(batch_id, &chunk));

        if batch_count >= max_batch_entries {
            db.write(std::mem::take(&mut lane_batch)).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
            batch_count = 0;
        }
        batch_id += 1;

        lanes_imported += chunk.len() as u64;
        progress.report(chunk.len());

        // After this chunk's feed, some lanes from earlier chunks may have
        // sealed (via `seal_up_to` / `chain_up` callbacks). Flush whatever's
        // resolvable and drop those lanes from the per-lane seal map.
        flush_resolved_pending(
            db,
            stores,
            block_hash,
            &mut pending,
            builder.sink_mut(),
            &mut score_batch,
            &mut score_batch_count,
            max_batch_entries,
        )?;
    }

    progress.report_completion();

    let (root, mut sink) = builder.finish()?;
    sink.flush_batch().map_err(StreamError::Sink)?;
    flush_lane_batch(db, lane_batch, batch_count)?;

    // After finalize, every leaf has sealed — any pending entry that
    // didn't resolve in-loop must resolve now.
    flush_resolved_pending(
        db,
        stores,
        block_hash,
        &mut pending,
        &mut sink,
        &mut score_batch,
        &mut score_batch_count,
        max_batch_entries,
    )?;
    debug_assert!(pending.is_empty(), "post-finalize: every pending score-index entry must be resolvable");
    if score_batch_count > 0 {
        db.write(score_batch).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
    }

    Ok(StreamingImportResult { root, lanes_imported, nodes_written: sink.nodes_written() })
}

/// One chunk's worth of score-index args, awaiting per-lane seal events.
struct PendingScoreIndex {
    batch_id: u32,
    /// Lane keys grouped by their blue_score, ordered by bs (BTreeMap drained into Vec).
    groups: Vec<(u64, Vec<Hash>)>,
}

impl PendingScoreIndex {
    fn from_chunk(batch_id: u32, chunk: &[ImportLane]) -> Self {
        let mut groups: BTreeMap<u64, Vec<Hash>> = BTreeMap::new();
        for lane in chunk {
            groups.entry(lane.blue_score).or_default().push(lane.lane_key);
        }
        Self { batch_id, groups: groups.into_iter().collect() }
    }

    /// True when every lane in every bs group has been sealed. Required
    /// before flushing — the score-index entry's `max_depth` must cover
    /// every lane that will share its `(bs, batch_id)` storage slot.
    fn is_resolvable(&self, sink: &DbSink<'_>) -> bool {
        self.groups.iter().all(|(_, lks)| lks.iter().all(|lk| sink.seal_depth_for(lk).is_some()))
    }

    /// Caller must have verified [`Self::is_resolvable`]; this consumes the
    /// entry's lane keys so the caller can hand them to `forget_lanes`.
    fn write_to_batch(
        &self,
        batch: &mut WriteBatch,
        stores: &SmtStores,
        block_hash: BlockHash,
        sink: &DbSink<'_>,
    ) -> Result<usize, StreamError<StoreError>> {
        let mut written = 0;
        for (bs, lane_keys) in &self.groups {
            // All lanes in this group are sealed (verified by is_resolvable).
            // Take the max seal depth across the group as the entry's `max_depth`.
            let max_depth = lane_keys.iter().filter_map(|lk| sink.seal_depth_for(lk)).max().unwrap_or(0);
            stores
                .score_index
                .put_batched(
                    BatchDbWriter::new(batch),
                    *bs,
                    ScoreIndexKind::LeafUpdate,
                    block_hash,
                    lane_keys,
                    self.batch_id,
                    max_depth,
                )
                .map_err(StreamError::Sink)?;
            written += 1;
        }
        Ok(written)
    }

    fn into_lane_keys(self) -> impl Iterator<Item = Hash> {
        self.groups.into_iter().flat_map(|(_, lks)| lks)
    }
}

/// Walk `pending` once, flushing every entry whose lanes are all sealed and
/// dropping those lanes from the sink's per-lane map. Out-of-order is fine —
/// each entry has a distinct `(batch_id, bs, ...)` storage slot.
fn flush_resolved_pending(
    db: &DB,
    stores: &SmtStores,
    block_hash: BlockHash,
    pending: &mut Vec<PendingScoreIndex>,
    sink: &mut DbSink<'_>,
    score_batch: &mut WriteBatch,
    score_batch_count: &mut usize,
    max_batch_entries: usize,
) -> Result<(), StreamError<StoreError>> {
    let mut i = 0;
    while i < pending.len() {
        if pending[i].is_resolvable(sink) {
            let entry = pending.swap_remove(i);
            let written = entry.write_to_batch(score_batch, stores, block_hash, sink)?;
            *score_batch_count += written;
            sink.forget_lanes(entry.into_lane_keys());
            if *score_batch_count >= max_batch_entries {
                db.write(std::mem::take(score_batch)).map_err(|e| StreamError::Sink(StoreError::DbError(e)))?;
                *score_batch_count = 0;
            }
            // Don't advance `i` — `swap_remove` moved a fresh entry into this slot.
        } else {
            i += 1;
        }
    }
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
