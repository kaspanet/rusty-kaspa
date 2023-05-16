use itertools::Itertools;
use kaspa_consensus_core::{blockhash::BlockHashes, BlockHashMap, BlockHasher, BlockLevel, HashMapCustomHasher};
use kaspa_database::prelude::DbWriter;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DbKey, DirectDbWriter};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::{collections::hash_map::Entry::Vacant, sync::Arc};

/// Reader API for `RelationsStore`.
pub trait RelationsStoreReader {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;

    /// Returns the counts of entries in parents/children stores. To be used for tests only
    fn counts(&self) -> Result<(usize, usize), StoreError>;
}

/// Write API for `RelationsStore`. The insert function is deliberately `mut`
/// since it modifies the children arrays for previously added parents which is
/// non-append-only and thus needs to be guarded.
pub trait RelationsStore: RelationsStoreReader {
    /// Inserts `parents` into a new store entry for `hash`, and for each `parent ∈ parents` adds `hash` to `parent.children`
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError>;
    fn delete(&mut self, hash: Hash) -> Result<(), StoreError>;
    fn replace_parent(&mut self, hash: Hash, replaced_parent: Hash, replace_with: &[Hash]) -> Result<(), StoreError>;
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
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_size: u64) -> Self {
        let lvl_bytes = level.to_le_bytes();
        let parents_prefix = PARENTS_PREFIX.iter().copied().chain(lvl_bytes).collect_vec();
        let children_prefix = CHILDREN_PREFIX.iter().copied().chain(lvl_bytes).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_access: CachedDbAccess::new(db, cache_size, children_prefix),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_size: u64) -> Self {
        let parents_prefix = prefix.iter().copied().chain(PARENTS_PREFIX.iter().copied()).collect_vec();
        let children_prefix = prefix.iter().copied().chain(CHILDREN_PREFIX.iter().copied()).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_access: CachedDbAccess::new(db, cache_size, children_prefix),
        }
    }

    fn insert_with_writer(&self, mut writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }

        // Insert a new entry for `hash`
        self.parents_access.write(&mut writer, hash, parents.clone())?;

        // The new hash has no children yet
        self.children_access.write(&mut writer, hash, BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access.write(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }

    /// Delete children and parents entries of `hash` and remove `hash` from `children` of each of its parents
    /// Note: the removal from parents of each child must be done beforehand by calling `replace_parent` for each child
    fn delete_with_writer(&self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        let parents = self.parents_access.read(hash)?;
        self.parents_access.delete(&mut writer, hash)?;
        self.children_access.delete(&mut writer, hash)?;

        // Remove `hash` from `children` of each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            let index = children.iter().copied().position(|h| h == hash).expect("inconsistent child-parent relation");
            children.swap_remove(index);
            self.children_access.write(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }

    fn replace_parent_with_writer(
        &self,
        mut writer: impl DbWriter,
        hash: Hash,
        replaced_parent: Hash,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        let mut parents = (*self.get_parents(hash)?).clone();
        let replaced_index =
            parents.iter().copied().position(|h| h == replaced_parent).expect("callers must ensure replaced is a parent");
        parents.swap_remove(replaced_index);
        parents.extend(replace_with);
        self.parents_access.write(&mut writer, hash, BlockHashes::new(parents))?;

        for parent in replace_with.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_access.write(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }

    pub fn insert_batch(&mut self, batch: &mut WriteBatch, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.insert_with_writer(BatchDbWriter::new(batch), hash, parents)
    }

    pub fn delete_batch(&mut self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.delete_with_writer(BatchDbWriter::new(batch), hash)
    }

    pub fn replace_parent_batch(
        &mut self,
        batch: &mut WriteBatch,
        hash: Hash,
        replaced_parent: Hash,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        self.replace_parent_with_writer(BatchDbWriter::new(batch), hash, replaced_parent, replace_with)
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.parents_access.read(hash)
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.children_access.read(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.parents_access.has(hash)? {
            debug_assert!(self.children_access.has(hash)?);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        Ok((self.parents_access.iterator().count(), self.children_access.iterator().count()))
    }
}

impl RelationsStore for DbRelationsStore {
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.insert_with_writer(DirectDbWriter::new(&self.db), hash, parents)
    }

    fn delete(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.delete_with_writer(DirectDbWriter::new(&self.db), hash)
    }

    fn replace_parent(&mut self, hash: Hash, replaced_parent: Hash, replace_with: &[Hash]) -> Result<(), StoreError> {
        self.replace_parent_with_writer(DirectDbWriter::new(&self.db), hash, replaced_parent, replace_with)
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

impl RelationsStoreReader for MemoryRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.parents_map.get(&hash) {
            Some(parents) => Ok(BlockHashes::clone(parents)),
            None => Err(StoreError::KeyNotFound(DbKey::new(PARENTS_PREFIX, hash))),
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.children_map.get(&hash) {
            Some(children) => Ok(BlockHashes::clone(children)),
            None => Err(StoreError::KeyNotFound(DbKey::new(CHILDREN_PREFIX, hash))),
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.parents_map.contains_key(&hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        Ok((self.parents_map.len(), self.children_map.len()))
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
                self.children_map.insert(parent, BlockHashes::new(children));
            }

            // The new hash has no children yet
            self.children_map.insert(hash, BlockHashes::new(Vec::new()));
            Ok(())
        } else {
            Err(StoreError::HashAlreadyExists(hash))
        }
    }

    fn delete(&mut self, hash: Hash) -> Result<(), StoreError> {
        let parents = self.get_parents(hash)?;
        self.parents_map.remove(&hash);
        self.children_map.remove(&hash);

        // Remove `hash` from `children` of each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            let index = children.iter().copied().position(|h| h == hash).expect("inconsistent child-parent relation");
            children.swap_remove(index);
            self.children_map.insert(parent, BlockHashes::new(children));
        }

        Ok(())
    }

    fn replace_parent(&mut self, hash: Hash, replaced_parent: Hash, replace_with: &[Hash]) -> Result<(), StoreError> {
        let mut parents = (*self.get_parents(hash)?).clone();
        let replaced_index =
            parents.iter().copied().position(|h| h == replaced_parent).expect("callers must ensure replaced is a parent");
        parents.swap_remove(replaced_index);
        parents.extend(replace_with);
        self.parents_map.insert(hash, BlockHashes::new(parents));

        for parent in replace_with.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.children_map.insert(parent, BlockHashes::new(children));
        }

        Ok(())
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
        let db_tempdir = kaspa_database::utils::get_kaspa_tempdir();
        let db = Arc::new(DB::open_default(db_tempdir.path().to_owned().to_str().unwrap()).unwrap());
        test_relations_store(DbRelationsStore::new(db, 0, 2));
    }

    fn test_relations_store<T: RelationsStore>(mut store: T) {
        let parents = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![2, 3]), (5, vec![1, 4])];
        for (i, vec) in parents.iter().cloned() {
            store.insert(i.into(), BlockHashes::new(vec.iter().copied().map(Hash::from).collect())).unwrap();
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
