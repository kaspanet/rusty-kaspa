use kaspa_database::prelude::DB;
use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DirectDbWriter, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

pub trait PchmrStoreReader {
    fn get(&self, hash: Hash) -> Result<Hash, StoreError>;
}

pub trait PchmrStore: PchmrStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, pchmr: Hash) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}
#[derive(Clone)]
pub struct DbPchmrStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Hash>,
}

impl DbPchmrStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PochmMerkleRoots.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl PchmrStore for DbPchmrStore {
    fn insert(&self, hash: Hash, pchmr: Hash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            if self.get(hash).unwrap() != pchmr
            //TODO:temporary workaround - this is bad and should not be kept this way
            {
                return Err(StoreError::HashAlreadyExists(hash));
            } else {
                return Ok(());
            }
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, pchmr)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

impl PchmrStoreReader for DbPchmrStore {
    fn get(&self, hash: Hash) -> Result<Hash, StoreError> {
        self.access.read(hash)
    }
}
