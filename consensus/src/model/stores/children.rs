use keryx_consensus_core::BlockHashSet;
use keryx_consensus_core::BlockHasher;
use keryx_consensus_core::BlockLevel;
use keryx_database::prelude::BatchDbWriter;
use keryx_database::prelude::CachePolicy;
use keryx_database::prelude::CachedDbSetAccess;
use keryx_database::prelude::DB;
use keryx_database::prelude::DbWriter;
use keryx_database::prelude::ReadLock;
use keryx_database::prelude::StoreError;
use keryx_database::prelude::StoreResult;
use keryx_database::registry::DatabaseStorePrefixes;
use keryx_hashes::Hash;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub trait ChildrenStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>>;
}

pub trait ChildrenStore {
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `DbChildrenStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbChildrenStore {
    _db: Arc<DB>,
    access: CachedDbSetAccess<Hash, Hash, BlockHasher, BlockHasher>,
}

impl DbChildrenStore {
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_policy: CachePolicy) -> Self {
        let lvl_bytes = level.to_le_bytes();
        Self {
            _db: Arc::clone(&db),
            access: CachedDbSetAccess::new(
                db,
                cache_policy,
                DatabaseStorePrefixes::RelationsChildren.into_iter().chain(lvl_bytes).collect(),
            ),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_policy: CachePolicy) -> Self {
        let db_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsChildren).collect();
        Self { _db: Arc::clone(&db), access: CachedDbSetAccess::new(db, cache_policy, db_prefix) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), parent, child)?;
        Ok(())
    }

    pub(crate) fn delete_children(&self, mut writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.access.delete_bucket(&mut writer, parent)
    }

    pub(crate) fn prefix(&self) -> &[u8] {
        self.access.prefix()
    }
}

impl ChildrenStoreReader for DbChildrenStore {
    fn get(&self, parent: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        self.access.read(parent)
    }
}

impl ChildrenStore for DbChildrenStore {
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.write(writer, parent, child)?;
        Ok(())
    }

    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.delete(writer, parent, child)
    }
}
