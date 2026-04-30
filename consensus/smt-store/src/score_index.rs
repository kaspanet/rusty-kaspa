use std::ops::RangeInclusive;
use std::sync::Arc;

use crate::keys::{BatchedScoreIndexKey, ScoreIndexKey, ScoreIndexKind, ScoreIndexValue};
use crate::maybe_fork::MaybeFork;
use crate::reacquire_iter::ReacquiringRawIterator;
use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use zerocopy::FromBytes;

/// Owned counterpart of [`ScoreIndexValue`], used by iterators that hand out
/// owned data (`MaybeFork`).
#[derive(Debug, Clone)]
pub struct ScoreIndexValueOwned {
    pub max_depth: u8,
    pub lane_keys: Vec<Hash>,
}

/// Append-only score index.
///
/// Records lane changes per block, keyed by `(rev_blue_score, kind, block_hash)`.
/// Each block may produce two entries:
/// - `LeafUpdate` — lanes inserted or updated (score = lane's blue_score)
/// - `Structural` — lanes expired by this block (score = block's blue_score),
///   recorded only so pruning can delete the branch_version entries on those
///   lanes' paths; expired lane_keys do not reappear in `LeafUpdate`.
///
/// These are **historical records** of what happened at a given score,
/// not a reflection of current lane state.
pub struct DbScoreIndex {
    db: Arc<DB>,
    prefix: u8,
}

impl DbScoreIndex {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db, prefix: DatabaseStorePrefixes::SmtScoreIndex.into() }
    }

    pub fn delete_all(&self) {
        use kaspa_database::prelude::DirectDbWriter;
        DirectDbWriter::new(&self.db).delete_range(vec![self.prefix], vec![self.prefix + 1]).unwrap();
    }

    /// Write lane keys for a block with the given change kind.
    ///
    /// `max_depth` is the deepest branch depth touched by this block; pruning
    /// uses it to bound the depth range of branch-version deletes.
    pub fn put(
        &self,
        mut writer: impl DbWriter,
        blue_score: u64,
        kind: ScoreIndexKind,
        block_hash: Hash,
        lane_keys: &[Hash],
        max_depth: u8,
    ) -> StoreResult<()> {
        let key = ScoreIndexKey::new(self.prefix, blue_score, kind, block_hash);
        let value = ScoreIndexValue::to_value_bytes(max_depth, lane_keys);
        writer.put(key, value).map_err(StoreError::DbError)
    }

    /// Write lane keys with a batch_id suffix to prevent key collisions during IBD.
    ///
    /// Uses [`BatchedScoreIndexKey`] (46 bytes) instead of [`ScoreIndexKey`] (42 bytes),
    /// ensuring unique keys when multiple chunks share the same `(blue_score, kind, block_hash)`.
    pub fn put_batched(
        &self,
        mut writer: impl DbWriter,
        blue_score: u64,
        kind: ScoreIndexKind,
        block_hash: Hash,
        lane_keys: &[Hash],
        batch_id: u32,
        max_depth: u8,
    ) -> StoreResult<()> {
        let key = BatchedScoreIndexKey::new(self.prefix, blue_score, kind, block_hash, batch_id);
        let value = ScoreIndexValue::to_value_bytes(max_depth, lane_keys);
        writer.put(key, value).map_err(StoreError::DbError)
    }

    pub fn count_entries_at_or_below(&self, cutoff_blue_score: u64) -> StoreResult<usize> {
        let prefix_bytes = [self.prefix];
        let mut iter = self.db.raw_iterator();
        iter.seek(prefix_bytes);

        let mut count = 0usize;
        while iter.valid() {
            let Some(key_bytes) = iter.key() else { break };
            if !key_bytes.starts_with(&prefix_bytes) {
                break;
            }
            let key = ScoreIndexKey::try_ref_from_key_bytes(key_bytes)
                .map_err(|e| StoreError::DataInconsistency(format!("score index key: {e}")))?;
            let blue_score = key.rev_blue_score.blue_score();
            if blue_score <= cutoff_blue_score {
                count += 1;
            }
            iter.next();
        }

        iter.status().map_err(StoreError::DbError)?;
        Ok(count)
    }

    /// Iterate `LeafUpdate` entries across the inclusive score band.
    ///
    /// Skips `Structural` entries. Used by expiration logic to find lanes whose
    /// last active update is falling out of the inactivity window.
    pub fn get_leaf_updates(
        &self,
        blue_score_range: RangeInclusive<u64>,
    ) -> impl Iterator<Item = StoreResult<MaybeFork<Vec<Hash>>>> + '_ {
        let min_blue_score = *blue_score_range.start();
        let target_blue_score = *blue_score_range.end();
        let seek_key = ScoreIndexKey::seek_key(self.prefix, target_blue_score);
        let score_prefix = [self.prefix];
        let prefix = self.prefix;

        let mut iter = self.db.raw_iterator();
        iter.seek(seek_key);

        let mut done = false;

        std::iter::from_fn(move || {
            loop {
                if done {
                    return None;
                }
                if !iter.valid() {
                    done = true;
                    return iter.status().err().map(|e| Err(StoreError::DbError(e)));
                }

                let Some(key_bytes) = iter.key() else {
                    done = true;
                    return None;
                };
                if !key_bytes.starts_with(&score_prefix) {
                    done = true;
                    return None;
                }
                let key = match ScoreIndexKey::try_ref_from_key_bytes(key_bytes) {
                    Ok(k) => k,
                    Err(e) => {
                        done = true;
                        return Some(Err(StoreError::DataInconsistency(format!("score index key: {e}"))));
                    }
                };
                let blue_score = key.rev_blue_score.blue_score();
                if blue_score < min_blue_score {
                    done = true;
                    return None;
                }
                if key.kind != ScoreIndexKind::LeafUpdate {
                    // Seek past the Structural group to the next lower score's LeafUpdate entries
                    if blue_score > min_blue_score {
                        iter.seek(ScoreIndexKey::seek_key(prefix, blue_score - 1));
                    } else {
                        done = true;
                    }
                    continue;
                }
                let Some(value_bytes) = iter.value() else {
                    done = true;
                    return None;
                };
                let view = match ScoreIndexValue::ref_from_bytes(value_bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        done = true;
                        return Some(Err(StoreError::DataInconsistency(format!("score index value: {e}"))));
                    }
                };
                let result = MaybeFork::new(view.lane_keys.to_vec(), blue_score, key.block_hash);
                iter.next();
                return Some(Ok(result));
            }
        })
    }

    /// Iterate **all** entries (both `LeafUpdate` and `Structural`) across the inclusive score band.
    ///
    /// Returns `(ScoreIndexValueOwned, blue_score, block_hash)` for pruning
    /// lane_version and branch_version stores. The owned value carries
    /// `max_depth` so pruning can bound branch deletes by the deepest depth
    /// touched by the block, and the lane keys for which to delete entries.
    /// The score index itself is pruned separately via [`delete_range`].
    ///
    /// Uses a reacquiring iterator; callers must ensure compatible consistency
    /// semantics for the scanned range while consuming the iterator.
    pub fn get_all(
        &self,
        blue_score_range: RangeInclusive<u64>,
    ) -> impl Iterator<Item = StoreResult<MaybeFork<ScoreIndexValueOwned>>> + '_ {
        let min_blue_score = *blue_score_range.start();
        let target_blue_score = *blue_score_range.end();
        let seek_key = ScoreIndexKey::seek_key(self.prefix, target_blue_score);
        let score_prefix = [self.prefix];

        let mut iter = ReacquiringRawIterator::new(&self.db);
        iter.seek(seek_key);

        let mut done = false;

        std::iter::from_fn(move || {
            if done {
                return None;
            }
            if !iter.valid() {
                done = true;
                return iter.status().err().map(|e| Err(StoreError::DbError(e)));
            }

            let result = (|| -> StoreResult<Option<MaybeFork<ScoreIndexValueOwned>>> {
                let Some(key_bytes) = iter.key() else { return Ok(None) };
                if !key_bytes.starts_with(&score_prefix) {
                    return Ok(None);
                }
                let key = ScoreIndexKey::try_ref_from_key_bytes(key_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index key: {e}")))?;
                let blue_score = key.rev_blue_score.blue_score();
                if blue_score < min_blue_score {
                    return Ok(None);
                }
                let Some(value_bytes) = iter.value() else { return Ok(None) };
                let view = ScoreIndexValue::ref_from_bytes(value_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index value: {e}")))?;
                let owned = ScoreIndexValueOwned { max_depth: view.max_depth, lane_keys: view.lane_keys.to_vec() };
                Ok(Some(MaybeFork::new(owned, blue_score, key.block_hash)))
            })();

            match result {
                Ok(Some(entry)) => {
                    iter.next();
                    Some(Ok(entry))
                }
                Ok(None) => {
                    done = true;
                    None
                }
                Err(e) => {
                    done = true;
                    Some(Err(e))
                }
            }
        })
    }

    /// Range delete for pruning: remove all entries with `actual_blue_score <= up_to_blue_score`.
    ///
    /// With [`ReverseBlueScore`], entries with low actual scores have high
    /// reversed values, so they appear at the END of the key space.
    /// We delete from `prefix | ReverseBlueScore(up_to_blue_score)` to `prefix_end`.
    pub fn delete_range(&self, mut writer: impl DbWriter, up_to_blue_score: u64) -> StoreResult<()> {
        let from_key = ScoreIndexKey::seek_key(self.prefix, up_to_blue_score);
        let from = from_key.as_ref().to_vec();
        let to = vec![self.prefix + 1];
        writer.delete_range(from, to).map_err(StoreError::DbError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, DirectDbWriter};

    fn make_store() -> (kaspa_database::utils::DbLifetime, DbScoreIndex) {
        let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        (lifetime, DbScoreIndex::new(db))
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    #[test]
    fn put_and_get_leaf_updates() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);
        let lanes = vec![hash(0x11), hash(0x22)];

        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &lanes, 0).unwrap();

        let results: Vec<_> = store.get_leaf_updates(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data().len(), 2);
    }

    #[test]
    fn get_leaf_updates_skips_structural() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x11)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::Structural, block, &[hash(0x22)], 0).unwrap();

        let updated: Vec<_> = store.get_leaf_updates(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].data(), &[hash(0x11)]);
    }

    #[test]
    fn get_all_returns_both_kinds() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x11)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::Structural, block, &[hash(0x22)], 0).unwrap();

        let all: Vec<_> = store.get_all(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].data().lane_keys, vec![hash(0x11)]);
        assert_eq!(all[1].data().lane_keys, vec![hash(0x22)]);
    }

    #[test]
    fn mixed_scores_leaf_updates_only() {
        let (_lt, store) = make_store();

        store.put(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::LeafUpdate, hash(0xA0), &[hash(0x01)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::Structural, hash(0xA0), &[hash(0x02)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, hash(0xA1), &[hash(0x03)], 0).unwrap();

        let updated: Vec<_> = store.get_leaf_updates(0..=200).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].data(), &[hash(0x03)]); // score 100 first (newest)
        assert_eq!(updated[1].data(), &[hash(0x01)]); // score 50 second
    }

    #[test]
    fn delete_range_prunes_both_kinds() {
        let (_lt, store) = make_store();

        store.put(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::LeafUpdate, hash(0xA0), &[hash(0x01)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::Structural, hash(0xA0), &[hash(0x02)], 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 200, ScoreIndexKind::LeafUpdate, hash(0xA2), &[hash(0x03)], 0).unwrap();

        store.delete_range(DirectDbWriter::new(&store.db), 100).unwrap();

        // Score 50 entries (both kinds) should be gone
        assert!(store.get_all(0..=100).next().is_none());

        // Score 200 should remain
        let results: Vec<_> = store.get_all(0..=200).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn put_batched_prevents_collision() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        // Two batched writes at same (score, kind, block_hash) with different batch_ids
        store.put_batched(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x11)], 0, 0).unwrap();
        store.put_batched(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x22)], 1, 0).unwrap();

        let results: Vec<_> = store.get_leaf_updates(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 2);
        // Both entries are present (order depends on batch_id suffix)
        let all_lanes: Vec<_> = results.iter().flat_map(|r| r.data().iter()).copied().collect();
        assert!(all_lanes.contains(&hash(0x11)));
        assert!(all_lanes.contains(&hash(0x22)));
    }

    #[test]
    fn mixed_key_lengths_readable() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        // Normal 42-byte key
        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x11)], 0).unwrap();
        // Batched 46-byte key at same score
        store.put_batched(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &[hash(0x22)], 0, 0).unwrap();

        let updated: Vec<_> = store.get_leaf_updates(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(updated.len(), 2);

        let all: Vec<_> = store.get_all(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn delete_range_handles_batched_entries() {
        let (_lt, store) = make_store();

        store.put_batched(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::LeafUpdate, hash(0xA0), &[hash(0x01)], 0, 0).unwrap();
        store.put_batched(DirectDbWriter::new(&store.db), 50, ScoreIndexKind::LeafUpdate, hash(0xA0), &[hash(0x02)], 1, 0).unwrap();
        store.put(DirectDbWriter::new(&store.db), 200, ScoreIndexKind::LeafUpdate, hash(0xA2), &[hash(0x03)], 0).unwrap();

        store.delete_range(DirectDbWriter::new(&store.db), 100).unwrap();

        // Batched score 50 entries should be gone
        assert!(store.get_all(0..=100).next().is_none());

        // Score 200 should remain
        let results: Vec<_> = store.get_all(0..=200).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn roundtrip_max_depth() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);
        let lanes = [hash(0x11), hash(0x22), hash(0x33)];

        store.put(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::LeafUpdate, block, &lanes, 42).unwrap();
        store.put_batched(DirectDbWriter::new(&store.db), 100, ScoreIndexKind::Structural, block, &lanes, 7, 200).unwrap();

        let all: Vec<_> = store.get_all(0..=100).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(all.len(), 2);
        let leaf = all.iter().find(|r| r.data().max_depth == 42).expect("LeafUpdate entry with max_depth=42");
        assert_eq!(leaf.data().lane_keys, lanes);
        let structural = all.iter().find(|r| r.data().max_depth == 200).expect("Structural entry with max_depth=200");
        assert_eq!(structural.data().lane_keys, lanes);
    }
}
