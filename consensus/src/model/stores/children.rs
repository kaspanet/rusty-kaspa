use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::BlockLevel;
use kaspa_database::prelude::BatchDbWriter;
use kaspa_database::prelude::CachedDbSetAccess;
use kaspa_database::prelude::DbWriter;
use kaspa_database::prelude::ReadLock;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_hashes::HASH_SIZE;
use rocksdb::WriteBatch;
use std::fmt::Display;
use std::sync::Arc;

pub trait ChildrenStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>>;
}

pub trait ChildrenStore {
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
}

struct ChildKey {
    parent: Hash,
    child: Hash,
}

const KEY_SIZE: usize = 2 * HASH_SIZE;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct DbChildKey([u8; KEY_SIZE]);

impl AsRef<[u8]> for DbChildKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for DbChildKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key: ChildKey = (*self).into();
        write!(f, "{}:{}", key.parent, key.child)
    }
}

impl From<ChildKey> for DbChildKey {
    fn from(key: ChildKey) -> Self {
        let mut bytes = [0; KEY_SIZE];
        bytes[..HASH_SIZE].copy_from_slice(&key.parent.as_bytes());
        bytes[HASH_SIZE..].copy_from_slice(&key.child.as_bytes());
        Self(bytes)
    }
}

impl From<DbChildKey> for ChildKey {
    fn from(k: DbChildKey) -> Self {
        let parent_bytes: [u8; HASH_SIZE] = k.0[..HASH_SIZE].try_into().unwrap();
        let parent: Hash = parent_bytes.into();

        let child_bytes: [u8; HASH_SIZE] = k.0[HASH_SIZE..].try_into().unwrap();
        let child: Hash = child_bytes.into();
        Self { parent, child }
    }
}

/// A DB + cache implementation of `DbChildrenStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbChildrenStore {
    db: Arc<DB>,
    access: CachedDbSetAccess<Hash, Hash, BlockHasher, BlockHasher>,
}

impl DbChildrenStore {
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_size: u64) -> Self {
        let lvl_bytes = level.to_le_bytes();
        Self {
            db: Arc::clone(&db),
            access: CachedDbSetAccess::new(
                db,
                cache_size,
                DatabaseStorePrefixes::RelationsChildren.into_iter().chain(lvl_bytes).collect(),
            ),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_size: u64) -> Self {
        let db_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsChildren).collect();
        Self { db: Arc::clone(&db), access: CachedDbSetAccess::new(db, cache_size, db_prefix) }
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

impl ChildrenStore for &DbChildrenStore {
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.write(writer, parent, child)?;
        Ok(())
    }

    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.delete(writer, parent, child)
    }
}
