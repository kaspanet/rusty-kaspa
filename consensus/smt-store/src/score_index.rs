use std::sync::Arc;

use crate::keys::{LaneChangeKind, ScoreIndexKey};
use crate::maybe_fork::MaybeFork;
use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

/// Append-only score index.
///
/// Records lane changes per block, keyed by `(rev_blue_score, block_hash, kind)`.
/// Each block may produce two entries:
/// - `kind=Updated` — lanes that received an active update
/// - `kind=Expired` — lanes that were expired (removed from the tree)
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
    pub fn put(
        &self,
        mut writer: impl DbWriter,
        blue_score: u64,
        kind: LaneChangeKind,
        block_hash: Hash,
        lane_keys: &[Hash],
    ) -> StoreResult<()> {
        let key = ScoreIndexKey::new(self.prefix, blue_score, kind, block_hash);
        let value = lane_keys.as_bytes();
        writer.put(key, value).map_err(StoreError::DbError)
    }

    /// Iterate **updated** lane records from `target_blue_score` downward.
    ///
    /// Returns historical records of lanes that received an active update
    /// at each score. Used by `expire_stale_lanes` to find lanes whose
    /// last active update is falling out of the inactivity window.
    pub fn get_updated(
        &self,
        target_blue_score: u64,
        min_blue_score: u64,
    ) -> impl Iterator<Item = StoreResult<MaybeFork<Vec<Hash>>>> + '_ {
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
                let key = match ScoreIndexKey::try_ref_from_bytes(key_bytes) {
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
                if key.kind != LaneChangeKind::Updated {
                    // Seek past the Expired group to the next lower score's Updated entries
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
                let lane_keys = match <[Hash]>::ref_from_bytes(value_bytes) {
                    Ok(k) => k,
                    Err(e) => {
                        done = true;
                        return Some(Err(StoreError::DataInconsistency(format!("score index value: {e}"))));
                    }
                };
                let result = MaybeFork::new(lane_keys.to_vec(), blue_score, key.block_hash);
                iter.next();
                return Some(Ok(result));
            }
        })
    }

    /// Iterate **all** lane change records (both updated and expired) from `target_blue_score` downward.
    ///
    /// Returns historical records of all lane changes. For pruning:
    /// - `Updated` records → delete lane + branch versions at that `(blue_score, block_hash)`
    /// - `Expired` records → delete branch versions only (no lane version was written)
    pub fn get_all(
        &self,
        target_blue_score: u64,
        min_blue_score: u64,
    ) -> impl Iterator<Item = StoreResult<(LaneChangeKind, MaybeFork<Vec<Hash>>)>> + '_ {
        let seek_key = ScoreIndexKey::seek_key(self.prefix, target_blue_score);
        let score_prefix = [self.prefix];

        let mut iter = self.db.raw_iterator();
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

            let result = (|| -> StoreResult<Option<(LaneChangeKind, MaybeFork<Vec<Hash>>)>> {
                let Some(key_bytes) = iter.key() else { return Ok(None) };
                if !key_bytes.starts_with(&score_prefix) {
                    return Ok(None);
                }
                let key = ScoreIndexKey::try_ref_from_bytes(key_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index key: {e}")))?;
                let blue_score = key.rev_blue_score.blue_score();
                if blue_score < min_blue_score {
                    return Ok(None);
                }
                let Some(value_bytes) = iter.value() else { return Ok(None) };
                let lane_keys = <[Hash]>::ref_from_bytes(value_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index value: {e}")))?;
                Ok(Some((key.kind, MaybeFork::new(lane_keys.to_vec(), blue_score, key.block_hash))))
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
    fn put_and_get_updated() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);
        let lanes = vec![hash(0x11), hash(0x22)];

        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Updated, block, &lanes).unwrap();

        let results: Vec<_> = store.get_updated(100, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data().len(), 2);
    }

    #[test]
    fn get_updated_skips_expired() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Updated, block, &[hash(0x11)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Expired, block, &[hash(0x22)]).unwrap();

        let updated: Vec<_> = store.get_updated(100, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].data(), &[hash(0x11)]);
    }

    #[test]
    fn get_all_returns_both_kinds() {
        let (_lt, store) = make_store();
        let block = hash(0xBB);

        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Updated, block, &[hash(0x11)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Expired, block, &[hash(0x22)]).unwrap();

        let all: Vec<_> = store.get_all(100, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, LaneChangeKind::Updated);
        assert_eq!(all[0].1.data(), &[hash(0x11)]);
        assert_eq!(all[1].0, LaneChangeKind::Expired);
        assert_eq!(all[1].1.data(), &[hash(0x22)]);
    }

    #[test]
    fn mixed_scores_updated_only() {
        let (_lt, store) = make_store();

        store.put(DirectDbWriter::new(&store.db), 50, LaneChangeKind::Updated, hash(0xA0), &[hash(0x01)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 50, LaneChangeKind::Expired, hash(0xA0), &[hash(0x02)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, LaneChangeKind::Updated, hash(0xA1), &[hash(0x03)]).unwrap();

        let updated: Vec<_> = store.get_updated(200, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].data(), &[hash(0x03)]); // score 100 first (newest)
        assert_eq!(updated[1].data(), &[hash(0x01)]); // score 50 second
    }

    #[test]
    fn delete_range_prunes_both_kinds() {
        let (_lt, store) = make_store();

        store.put(DirectDbWriter::new(&store.db), 50, LaneChangeKind::Updated, hash(0xA0), &[hash(0x01)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 50, LaneChangeKind::Expired, hash(0xA0), &[hash(0x02)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 200, LaneChangeKind::Updated, hash(0xA2), &[hash(0x03)]).unwrap();

        store.delete_range(DirectDbWriter::new(&store.db), 100).unwrap();

        // Score 50 entries (both kinds) should be gone
        assert!(store.get_all(100, 0).next().is_none());

        // Score 200 should remain
        let results: Vec<_> = store.get_all(200, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
    }
}
