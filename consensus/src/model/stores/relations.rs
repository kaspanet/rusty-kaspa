use super::{
    common::BlockLevelWithHashKey,
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::{blockhash::BlockHashes, BlockHashMap, BlockHasher, HashMapCustomHasher, BlockLevel};
use hashes::Hash;
use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `RelationsStore`.
pub trait RelationsStoreReader {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_parents_by_level(&self, hash: Hash, level: BlockLevel) -> Result<BlockHashes, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_children_by_level(&self, hash: Hash, level: BlockLevel) -> Result<BlockHashes, StoreError>;
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
    fn has_by_level(&self, hash: Hash, level: BlockLevel) -> Result<bool, StoreError>;
}

/// Write API for `RelationsStore`. The insert function is deliberately `mut`
/// since it modifies the children arrays for previously added parents which is
/// non-append-only and thus needs to be guarded.
pub trait RelationsStore: RelationsStoreReader {
    /// Inserts `parents` into a new store entry for `hash`, and for each `parent ∈ parents` adds `hash` to `parent.children`
    fn insert(&mut self, hash: Hash, level: BlockLevel, parents: BlockHashes) -> Result<(), StoreError>;
}

const PARENTS_PREFIX: &[u8] = b"block-parents";
const CHILDREN_PREFIX: &[u8] = b"block-children";

/// A DB + cache implementation of `RelationsStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbRelationsStore {
    db: Arc<DB>,
    parents_access: CachedDbAccess<Hash, Arc<Vec<Hash>>, BlockHasher>,
    children_access: CachedDbAccess<Hash, Arc<Vec<Hash>>, BlockHasher>,
}

impl DbRelationsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, PARENTS_PREFIX),
            children_access: CachedDbAccess::new(db, cache_size, CHILDREN_PREFIX),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    // Should be kept private and used only through `RelationsStoreBatchExtensions.insert_batch`
    fn insert_batch(&mut self, batch: &mut WriteBatch, hash: Hash, level: BlockLevel, parents: BlockHashes) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }

        let key = (level, hash).into();

        // Insert a new entry for `hash`
        self.parents_access.write(BatchDbWriter::new(batch), hash, parents.clone())?;

        // The new hash has no children yet
        self.children_access.write(BatchDbWriter::new(batch), hash, BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access.write(BatchDbWriter::new(batch), parent, BlockHashes::new(children))?;
        }

        Ok(())
    }
}

pub trait RelationsStoreBatchExtensions {
    fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        level: BlockLevel,
        parents: BlockHashes,
    ) -> Result<RwLockWriteGuard<DbRelationsStore>, StoreError>;
}

impl RelationsStoreBatchExtensions for Arc<RwLock<DbRelationsStore>> {
    fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        level: BlockLevel,
        parents: BlockHashes,
    ) -> Result<RwLockWriteGuard<DbRelationsStore>, StoreError> {
        let mut write_guard = self.write();
        write_guard.insert_batch(batch, hash, level, parents)?;
        Ok(write_guard)
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.get_parents_by_level(hash, 0)
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.get_children_by_level(hash, 0)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.has_by_level(hash, 0)
    }

    fn get_parents_by_level(&self, hash: Hash, level: BlockLevel) -> Result<BlockHashes, StoreError> {
        self.parents_access.read((level, hash).into())
    }

    fn get_children_by_level(&self, hash: Hash, level: BlockLevel) -> Result<BlockHashes, StoreError> {
        self.children_access.read((level, hash).into())
    }

    fn has_by_level(&self, hash: Hash, level: BlockLevel) -> Result<bool, StoreError> {
        let key = (level, hash).into();
        if self.parents_access.has(key)? {
            debug_assert!(self.children_access.has(key)?);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl RelationsStore for DbRelationsStore {
    /// See `insert_batch` as well
    /// TODO: use one function with DbWriter for both this function and insert_batch
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }

        let key = (level, hash).into();
        // Insert a new entry for `hash`
        self.parents_access.write(DirectDbWriter::new(&self.db), hash, parents.clone())?;

        // The new hash has no children yet
        self.children_access.write(DirectDbWriter::new(&self.db), hash, BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access.write(DirectDbWriter::new(&self.db), parent, BlockHashes::new(children))?;
        }

        Ok(())
    }
}

pub struct MemoryRelationsStore {
    parents_map: BlockHashMap<BlockHashes>,
    children_map: BlockHashMap<BlockHashes>,
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Self { parents_map: BlockHashMap::new(), children_map: BlockHashMap::new() }
    }
}

impl Default for MemoryRelationsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_relations_store() {
        let db_tempdir = tempfile::tempdir().unwrap();
        let db = Arc::new(DB::open_default(db_tempdir.path().to_owned().to_str().unwrap()).unwrap());
        test_relations_store(DbRelationsStore::new(db, 2));
    }

    fn test_relations_store<T: RelationsStore>(mut store: T) {
        let parents = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![2, 3]), (5, vec![1, 4])];
        for (i, vec) in parents.iter().cloned() {
            store.insert(i.into(), 0, BlockHashes::new(vec.iter().copied().map(Hash::from).collect())).unwrap();
        }

        let expected_children = [(1, vec![2, 3, 5]), (2, vec![4]), (3, vec![4]), (4, vec![5]), (5, vec![])];
        for (i, vec) in expected_children {
            assert!(store.get_children(i.into()).unwrap().iter().copied().eq(vec.iter().copied().map(Hash::from)));
        }

        for (i, vec) in parents {
            assert!(store.get_parents(i.into()).unwrap().iter().copied().eq(vec.iter().copied().map(Hash::from)));
        }
    }
}
