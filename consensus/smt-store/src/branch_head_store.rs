use std::sync::Arc;

use crate::keys::BranchHeadKey;
use kaspa_database::prelude::{DB, DbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;

pub struct DbBranchHeadStore {
    db: Arc<DB>,
    prefix: u8,
}

impl DbBranchHeadStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db, prefix: DatabaseStorePrefixes::SmtBranchHeads.into() }
    }

    pub fn get(&self, height: u8, node_key: Hash) -> StoreResult<Option<Hash>> {
        let key = BranchHeadKey::new(self.prefix, height, node_key);
        self.db
            .get_pinned(key)?
            .map(|slice| {
                Hash::try_from_slice(&slice)
                    .map_err(|_| StoreError::DataInconsistency(format!("branch head: expected 32 bytes, got {}", slice.len())))
            })
            .transpose()
    }

    pub fn set(&self, mut writer: impl DbWriter, height: u8, node_key: Hash, block_hash: Hash) -> StoreResult<()> {
        let key = BranchHeadKey::new(self.prefix, height, node_key);
        writer.put(key, block_hash.as_bytes()).map_err(StoreError::DbError)
    }

    pub fn delete(&self, mut writer: impl DbWriter, height: u8, node_key: Hash) -> StoreResult<()> {
        let key = BranchHeadKey::new(self.prefix, height, node_key);
        writer.delete(key).map_err(StoreError::DbError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, DirectDbWriter};

    fn make_store() -> (kaspa_database::utils::DbLifetime, DbBranchHeadStore) {
        let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        (lifetime, DbBranchHeadStore::new(db))
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    #[test]
    fn round_trip() {
        let (_lt, store) = make_store();
        let node_key = hash(0x11);
        let block_hash = hash(0x22);

        assert!(store.get(5, node_key).unwrap().is_none());
        store.set(DirectDbWriter::new(&store.db), 5, node_key, block_hash).unwrap();
        assert_eq!(store.get(5, node_key).unwrap(), Some(block_hash));
    }

    #[test]
    fn delete_head() {
        let (_lt, store) = make_store();
        let node_key = hash(0x11);
        let block_hash = hash(0x22);

        store.set(DirectDbWriter::new(&store.db), 5, node_key, block_hash).unwrap();
        assert!(store.get(5, node_key).unwrap().is_some());

        store.delete(DirectDbWriter::new(&store.db), 5, node_key).unwrap();
        assert!(store.get(5, node_key).unwrap().is_none());
    }
}
