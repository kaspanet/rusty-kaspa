use super::{caching::CachedDbAccess, errors::StoreError, DB};
use consensus_core::blockhash::BlockHashes;
use hashes::Hash;
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    sync::Arc,
};

pub trait RelationsStoreReader {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
}

pub trait RelationsStore: RelationsStoreReader {
    /// Inserts `parents` into a new store entry for `hash`, and for each `parent ∈ parents` adds `hash` to `parent.children`  
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError>;
}

const PARENTS_PREFIX: &[u8] = b"block-parents";
const CHILDREN_PREFIX: &[u8] = b"block-children";

#[derive(Clone)]
pub struct DbRelationsStore {
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    parents_access: CachedDbAccess<Hash, Vec<Hash>>,
    children_access: CachedDbAccess<Hash, Vec<Hash>>,
}

impl DbRelationsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, PARENTS_PREFIX),
            children_access: CachedDbAccess::new(Arc::clone(&db), cache_size, CHILDREN_PREFIX),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&self.raw_db),
            parents_access: CachedDbAccess::new(Arc::clone(&self.raw_db), cache_size, PARENTS_PREFIX),
            children_access: CachedDbAccess::new(Arc::clone(&self.raw_db), cache_size, CHILDREN_PREFIX),
        }
    }

    pub fn insert_batch(&mut self, batch: &mut WriteBatch, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }

        // Insert a new entry for `hash`
        self.parents_access
            .write_batch(batch, hash, &parents)?;

        // The new hash has no children yet
        self.children_access
            .write_batch(batch, hash, &BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access
                .write_batch(batch, parent, &BlockHashes::new(children))?;
        }

        Ok(())
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.parents_access.read(hash)?))
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.children_access.read(hash)?))
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.parents_access.has(hash)? {
            debug_assert!(self.children_access.has(hash)?);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl RelationsStore for DbRelationsStore {
    /// See `insert_batch` as well
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }

        // Insert a new entry for `hash`
        self.parents_access.write(hash, &parents)?;

        // The new hash has no children yet
        self.children_access
            .write(hash, &BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access
                .write(parent, &BlockHashes::new(children))?;
        }

        Ok(())
    }
}

pub struct MemoryRelationsStore {
    parents_map: HashMap<Hash, BlockHashes>,
    children_map: HashMap<Hash, BlockHashes>,
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Self { parents_map: HashMap::new(), children_map: HashMap::new() }
    }
}

impl Default for MemoryRelationsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationsStoreReader for MemoryRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.parents_map.get(&hash) {
            Some(parents) => Ok(BlockHashes::clone(parents)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.children_map.get(&hash) {
            Some(children) => Ok(BlockHashes::clone(children)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.parents_map.contains_key(&hash))
    }
}

impl RelationsStore for MemoryRelationsStore {
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        if let Vacant(e) = self.parents_map.entry(hash) {
            // Update the new entry for `hash`
            e.insert(BlockHashes::clone(&parents));

            // Update `children` for each parent
            for parent in parents.iter().cloned() {
                let mut children = (*self.get_children(parent)?).clone();
                children.push(hash);
                self.children_map
                    .insert(parent, BlockHashes::new(children));
            }

            // The new hash has no children yet
            self.children_map
                .insert(hash, BlockHashes::new(Vec::new()));
            Ok(())
        } else {
            Err(StoreError::KeyAlreadyExists(hash.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_relations_store() {
        test_relations_store(MemoryRelationsStore::new());
    }

    #[test]
    fn test_db_relations_store() {
        let db_tempdir = tempfile::tempdir().unwrap();
        let db = Arc::new(DB::open_default(db_tempdir.path().to_owned().to_str().unwrap()).unwrap());
        test_relations_store(DbRelationsStore::new(db, 2));
    }

    fn test_relations_store<T: RelationsStore>(mut store: T) {
        let parents = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![2, 3]), (5, vec![1, 4])];
        for (i, vec) in parents.iter().cloned() {
            store
                .insert(i.into(), BlockHashes::new(vec.iter().cloned().map(|j| j.into()).collect()))
                .unwrap();
        }

        let expected_children = [(1, vec![2, 3, 5]), (2, vec![4]), (3, vec![4]), (4, vec![5]), (5, vec![])];
        for (i, vec) in expected_children {
            assert!(store
                .get_children(i.into())
                .unwrap()
                .iter()
                .cloned()
                .eq(vec.iter().cloned().map(|j| j.into())));
        }

        for (i, vec) in parents {
            assert!(store
                .get_parents(i.into())
                .unwrap()
                .iter()
                .cloned()
                .eq(vec.iter().cloned().map(|j| j.into())));
        }
    }
}
