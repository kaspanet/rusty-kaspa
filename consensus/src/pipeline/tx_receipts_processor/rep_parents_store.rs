use kaspa_database::prelude::DB;
use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DirectDbWriter, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

pub trait RepParentsStoreReader {
    fn get(&self, hash: Hash) -> Result<Vec<Hash>, StoreError>;

    fn get_ith_rep_parent(&self, hash: Hash, ind: usize) -> Option<Hash>;
}

pub trait RepParentsStore: RepParentsStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, rep_parents_list: Vec<Hash>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}
#[derive(Clone)]
pub struct DbRepParentsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Vec<Hash>>,
}

impl DbRepParentsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::RepParentsList.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl RepParentsStore for DbRepParentsStore {
    fn insert(&self, hash: Hash, rep_parents_list: Vec<Hash>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, rep_parents_list)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

impl RepParentsStoreReader for DbRepParentsStore {
    fn get(&self, hash: Hash) -> Result<Vec<Hash>, StoreError> {
        self.access.read(hash)
    }
    fn get_ith_rep_parent(&self, hash: Hash, ind: usize) -> Option<Hash> {
        if let Ok(vec) = self.get(hash) {
            vec.get(ind).cloned()
        } else {
            None
        }
    }
}
