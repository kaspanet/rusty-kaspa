use std::sync::Arc;

use crate::keys::LaneHeadKey;
use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;

/// Lane Head Pointers.
///
/// One entry per lane that has ever been active. Points to the current
/// canonical version by `block_hash` only.
pub struct DbLaneHeadStore {
    db: Arc<DB>,
    prefix: u8,
}

impl DbLaneHeadStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db, prefix: DatabaseStorePrefixes::SmtLaneHeads.into() }
    }

    pub fn get(&self, lane_key: Hash) -> StoreResult<Option<Hash>> {
        let key = LaneHeadKey::new(self.prefix, lane_key);
        self.db
            .get_pinned(key)?
            .map(|slice| {
                Hash::try_from_slice(&slice)
                    .map_err(|_| StoreError::DataInconsistency(format!("lane head: expected 32 bytes, got {}", slice.len())))
            })
            .transpose()
    }

    pub fn set(&self, mut writer: impl DbWriter, lane_key: Hash, block_hash: Hash) -> StoreResult<()> {
        let key = LaneHeadKey::new(self.prefix, lane_key);
        writer.put(key, block_hash.as_bytes()).map_err(StoreError::DbError)
    }

    pub fn delete(&self, mut writer: impl DbWriter, lane_key: Hash) -> StoreResult<()> {
        let key = LaneHeadKey::new(self.prefix, lane_key);
        writer.delete(key).map_err(StoreError::DbError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, DirectDbWriter};

    fn make_store() -> (kaspa_database::utils::DbLifetime, DbLaneHeadStore) {
        let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        (lifetime, DbLaneHeadStore::new(db))
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    #[test]
    fn round_trip() {
        let (_lt, store) = make_store();
        let lane_key = hash(0x11);
        let block_hash = hash(0x22);

        assert!(store.get(lane_key).unwrap().is_none());
        store.set(DirectDbWriter::new(&store.db), lane_key, block_hash).unwrap();
        assert_eq!(store.get(lane_key).unwrap(), Some(block_hash));
    }

    #[test]
    fn delete_head() {
        let (_lt, store) = make_store();
        let lane_key = hash(0x11);
        let block_hash = hash(0x22);

        store.set(DirectDbWriter::new(&store.db), lane_key, block_hash).unwrap();
        assert!(store.get(lane_key).unwrap().is_some());

        store.delete(DirectDbWriter::new(&store.db), lane_key).unwrap();
        assert!(store.get(lane_key).unwrap().is_none());
    }
}
