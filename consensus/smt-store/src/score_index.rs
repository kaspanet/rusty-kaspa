use std::sync::Arc;

use crate::keys::ScoreIndexKey;
use crate::maybe_fork::MaybeFork;
use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use zerocopy::{FromBytes, IntoBytes};

/// Append-only score index.
///
/// Records all lane touches per block, keyed by `(rev_blue_score, block_hash)`.
/// Value is concatenated lane keys (`N * 32` bytes).
/// Entries are never deleted on rollback — pruned by score range only.
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

    /// Write all lane touches for a block at once.
    pub fn put(&self, mut writer: impl DbWriter, blue_score: u64, block_hash: Hash, lane_keys: &[Hash]) -> StoreResult<()> {
        let key = ScoreIndexKey::new(self.prefix, blue_score, block_hash);
        let value = lane_keys.as_bytes();
        writer.put(key, value).map_err(StoreError::DbError)
    }

    /// Iterate score index entries from `target_blue_score` downward.
    ///
    /// Returns `MaybeFork<Vec<Hash>>` — the `Vec<Hash>` contains lane keys
    /// touched by that block, and `MaybeFork` carries the `block_hash` for
    /// canonicality verification.
    pub fn get_at(&self, target_blue_score: u64, min_blue_score: u64) -> impl Iterator<Item = StoreResult<MaybeFork<Vec<Hash>>>> + '_ {
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

            let result = (|| -> StoreResult<Option<MaybeFork<Vec<Hash>>>> {
                let key_bytes = match iter.key() {
                    Some(k) => k,
                    None => return Ok(None),
                };

                if !key_bytes.starts_with(&score_prefix) {
                    return Ok(None);
                }

                let key = ScoreIndexKey::ref_from_bytes(key_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index key: {e}")))?;
                let blue_score = key.rev_blue_score.blue_score();
                debug_assert!(blue_score <= target_blue_score);

                if blue_score < min_blue_score {
                    return Ok(None);
                }

                let value_bytes = match iter.value() {
                    Some(v) => v,
                    None => return Ok(None),
                };

                let lane_keys = <[Hash]>::ref_from_bytes(value_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("score index value: {e}")))?;

                Ok(Some(MaybeFork::new(lane_keys.to_vec(), blue_score, key.block_hash)))
            })();

            match result {
                Ok(Some(fork)) => {
                    iter.next();
                    Some(Ok(fork))
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

        // Upper bound: increment prefix byte
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
    fn put_and_get_at() {
        let (_lt, store) = make_store();
        let block_b = hash(0xBB);
        let lanes = vec![hash(0x11), hash(0x22), hash(0x33)];

        store.put(DirectDbWriter::new(&store.db), 100, block_b, &lanes).unwrap();

        let block_other = hash(0xCC);
        store.put(DirectDbWriter::new(&store.db), 100, block_other, &[hash(0x44)]).unwrap();

        // get_at(100) should yield both blocks at score 100
        let results: Vec<_> = store.get_at(100, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 2);

        // Find the one from block_b
        let entry = results.iter().find(|r| r.block_hash() == block_b).unwrap();
        assert_eq!(entry.data().len(), 3);
        assert!(entry.data().contains(&hash(0x11)));
        assert!(entry.data().contains(&hash(0x22)));
        assert!(entry.data().contains(&hash(0x33)));

        // Find the one from block_other
        let entry = results.iter().find(|r| r.block_hash() == block_other).unwrap();
        assert_eq!(entry.data().len(), 1);
        assert_eq!(entry.data()[0], hash(0x44));

        // get_at(99) should yield nothing
        assert!(store.get_at(99, 0).next().is_none());
    }

    #[test]
    fn get_at_iterates_downward() {
        let (_lt, store) = make_store();

        store.put(DirectDbWriter::new(&store.db), 50, hash(0xA0), &[hash(0x01)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 100, hash(0xA1), &[hash(0x02)]).unwrap();
        store.put(DirectDbWriter::new(&store.db), 200, hash(0xA2), &[hash(0x03)]).unwrap();

        // target_blue_score=150 → yields score=100 then score=50
        let results: Vec<_> = store.get_at(150, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].block_hash(), hash(0xA1));
        assert_eq!(results[1].block_hash(), hash(0xA0));

        // target_blue_score=200 → all 3
        let results: Vec<_> = store.get_at(200, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn delete_range_prunes_old_entries() {
        let (_lt, store) = make_store();

        for score in [10u64, 50, 100, 200] {
            store.put(DirectDbWriter::new(&store.db), score, hash(score as u8), &[hash(score as u8)]).unwrap();
        }

        store.delete_range(DirectDbWriter::new(&store.db), 100).unwrap();

        // Scores 10, 50, 100 should be gone
        assert!(store.get_at(100, 0).next().is_none());

        // Score 200 should remain
        let results: Vec<_> = store.get_at(200, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data()[0], hash(200u8));
    }
}
