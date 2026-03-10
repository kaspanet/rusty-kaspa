use std::sync::Arc;

use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use zerocopy::{FromBytes, IntoBytes};

use crate::keys::LaneVersionKey;
use crate::maybe_fork::{MaybeFork, Verified};
use crate::values::LaneVersion;

/// Lane Versions.
///
/// One immutable entry per `(lane, block)` pair where the lane was
/// touched. Written once, never modified.
pub struct DbLaneVersionStore {
    db: Arc<DB>,
    prefix: u8,
}

impl DbLaneVersionStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db, prefix: DatabaseStorePrefixes::SmtLaneVersions.into() }
    }

    pub fn put(
        &self,
        mut writer: impl DbWriter,
        lane_key: Hash,
        blue_score: u64,
        block_hash: Hash,
        value: &LaneVersion,
    ) -> StoreResult<()> {
        let key = LaneVersionKey::new(self.prefix, lane_key, blue_score, block_hash);
        writer.put(key, value.as_bytes()).map_err(StoreError::DbError)
    }

    pub fn delete(&self, mut writer: impl DbWriter, lane_key: Hash, blue_score: u64, block_hash: Hash) -> StoreResult<()> {
        let key = LaneVersionKey::new(self.prefix, lane_key, blue_score, block_hash);
        writer.delete(key).map_err(StoreError::DbError)
    }

    /// Find the latest canonical version with `score >= min_blue_score`.
    ///
    /// Iterates from the highest score downward, stopping at `min_blue_score`.
    /// Returns the first entry where `is_canonical(block_hash)` is true.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = store.get(lane_key, 900, |bh| reachability.is_chain_ancestor(bh, tip));
    /// if let Some(version) = result? {
    ///     println!("lane_id={:?}, tip_hash={}", version.data().lane_id, version.data().lane_tip_hash);
    /// }
    /// ```
    pub fn get(
        &self,
        lane_key: Hash,
        min_blue_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> StoreResult<Option<Verified<LaneVersion>>> {
        for entry in self.get_at(lane_key, u64::MAX, min_blue_score) {
            let entry = entry?;
            if is_canonical(entry.block_hash()) {
                return Ok(Some(entry.into_verified()));
            }
        }
        Ok(None)
    }

    /// Iterate versions for `lane_key` from `target_blue_score` downward.
    ///
    /// Returns `MaybeFork<LaneVersion>` carrying both `score` and
    /// `block_hash` from the key. Caller verifies canonicality and
    /// picks the first match.
    pub fn get_at(
        &self,
        lane_key: Hash,
        target_blue_score: u64,
        min_blue_score: u64,
    ) -> impl Iterator<Item = StoreResult<MaybeFork<LaneVersion>>> + '_ {
        let seek_key = LaneVersionKey::seek_key(self.prefix, lane_key, target_blue_score);
        let mut entity_prefix = [0u8; LaneVersionKey::ENTITY_PREFIX_LEN];
        entity_prefix.copy_from_slice(&seek_key.as_ref()[..LaneVersionKey::ENTITY_PREFIX_LEN]);

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

            let result = (|| -> StoreResult<Option<MaybeFork<LaneVersion>>> {
                let key_bytes = match iter.key() {
                    Some(k) => k,
                    None => return Ok(None),
                };

                if !key_bytes.starts_with(&entity_prefix) {
                    return Ok(None);
                }

                let key = LaneVersionKey::ref_from_bytes(key_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("lane version key: {e}")))?;
                let blue_score = key.rev_blue_score.blue_score();
                debug_assert!(blue_score <= target_blue_score);

                if blue_score < min_blue_score {
                    return Ok(None);
                }

                let value_bytes = match iter.value() {
                    Some(v) => v,
                    None => return Ok(None),
                };
                let version = LaneVersion::read_from_bytes(value_bytes)
                    .map_err(|e| StoreError::DataInconsistency(format!("lane version value: {e}")))?;
                Ok(Some(MaybeFork::new(version, blue_score, key.block_hash)))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, DirectDbWriter};

    fn make_store() -> (kaspa_database::utils::DbLifetime, DbLaneVersionStore) {
        let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        (lifetime, DbLaneVersionStore::new(db))
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    #[test]
    fn put_and_get_at() {
        let (_lt, store) = make_store();
        let entry = LaneVersion { lane_id: [0x55; 20], lane_tip_hash: hash(0x66) };

        store.put(DirectDbWriter::new(&store.db), hash(0x11), 100, hash(0x22), &entry).unwrap();

        let first = store.get_at(hash(0x11), 100, 0).next().unwrap().unwrap();
        assert_eq!(first.block_hash(), hash(0x22));
        assert_eq!(first.blue_score(), 100);
        assert_eq!(first.data().lane_id, [0x55; 20]);
        assert_eq!(first.data().lane_tip_hash, hash(0x66));
    }

    #[test]
    fn get_at_iterates_versions() {
        let (_lt, store) = make_store();
        let lane_key = hash(0x11);

        for (score, bh) in [(50, hash(0xA0)), (100, hash(0xA1)), (200, hash(0xA2))] {
            let entry = LaneVersion { lane_id: [score as u8; 20], lane_tip_hash: hash(0xFF) };
            store.put(DirectDbWriter::new(&store.db), lane_key, score, bh, &entry).unwrap();
        }

        // target_blue_score=150 → score=100 then score=50
        let results: Vec<_> = store.get_at(lane_key, 150, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].block_hash(), hash(0xA1));
        assert_eq!(results[0].blue_score(), 100);
        assert_eq!(results[1].block_hash(), hash(0xA0));
        assert_eq!(results[1].blue_score(), 50);

        // target_blue_score=200 → all 3
        let results: Vec<_> = store.get_at(lane_key, 200, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 3);

        // target_blue_score=49 → nothing
        assert!(store.get_at(lane_key, 49, 0).next().is_none());
    }

    #[test]
    fn delete_entry() {
        let (_lt, store) = make_store();
        let entry = LaneVersion { lane_id: [0x55; 20], lane_tip_hash: hash(0x66) };
        store.put(DirectDbWriter::new(&store.db), hash(0x11), 100, hash(0x22), &entry).unwrap();

        assert!(store.get_at(hash(0x11), 100, 0).next().is_some());

        store.delete(DirectDbWriter::new(&store.db), hash(0x11), 100, hash(0x22)).unwrap();

        assert!(store.get_at(hash(0x11), 100, 0).next().is_none());
    }

    #[test]
    fn get_with_canonicality_filter() {
        let (_lt, store) = make_store();
        let lane_key = hash(0x11);

        let canonical_bh = hash(0xA1);
        let fork_bh = hash(0xA2);
        let older_bh = hash(0xA0);

        let mk = |id: u8| LaneVersion { lane_id: [id; 20], lane_tip_hash: hash(0xFF) };

        store.put(DirectDbWriter::new(&store.db), lane_key, 100, canonical_bh, &mk(0xCC)).unwrap();
        store.put(DirectDbWriter::new(&store.db), lane_key, 100, fork_bh, &mk(0xDD)).unwrap();
        store.put(DirectDbWriter::new(&store.db), lane_key, 50, older_bh, &mk(0xEE)).unwrap();

        // Finds canonical at score 100 (searching from MAX down to 0)
        let result = store.get(lane_key, 0, |bh| bh == canonical_bh).unwrap().unwrap();
        assert_eq!(result.block_hash(), canonical_bh);
        assert_eq!(result.blue_score(), 100);
        assert_eq!(result.data().lane_id, [0xCC; 20]);

        // Falls through to score 50 when score-100 blocks aren't canonical
        let result = store.get(lane_key, 0, |bh| bh == older_bh).unwrap().unwrap();
        assert_eq!(result.blue_score(), 50);

        // min_blue_score=60 excludes score 50
        assert!(store.get(lane_key, 60, |bh| bh == older_bh).unwrap().is_none());

        // No match at all
        assert!(store.get(lane_key, 0, |_| false).unwrap().is_none());
    }
}
