use itertools::Itertools;
use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::{blockhash::BlockHashes, BlockHashMap, BlockHasher, BlockLevel};
use kaspa_database::prelude::{BatchDbWriter, DbWriter, StoreResultExtensions};
use kaspa_database::prelude::{CachedDbAccess, DbKey, DirectDbWriter};
use kaspa_database::prelude::{DirectWriter, MemoryWriter};
use kaspa_database::prelude::{ReadLock, StoreError};
use kaspa_database::prelude::{StoreResult, DB};
use kaspa_database::registry::{DatabaseStorePrefixes, SEPARATOR};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::collections::hash_map::Entry;
use std::collections::HashSet;
use std::iter::once;
use std::sync::Arc;

use super::children::{ChildrenStore, ChildrenStoreReader, DbChildrenStore};

/// Reader API for `RelationsStore`.
pub trait RelationsStoreReader {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>>;
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;

    /// Returns the counts of entries in parents/children stores. To be used for tests only
    fn counts(&self) -> Result<(usize, usize), StoreError>;
}

impl<T: RelationsStoreReader> RelationsStoreReader for &T {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        (*self).get_parents(hash)
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        (*self).get_children(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        (*self).has(hash)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        (*self).counts()
    }
}

/// Low-level write API for `RelationsStore`
pub trait RelationsStore: RelationsStoreReader {
    type DefaultWriter: DirectWriter;
    fn default_writer(&self) -> Self::DefaultWriter;

    fn set_parents(&mut self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError>;
    fn delete_entries(&mut self, writer: impl DbWriter, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `RelationsStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbRelationsStore {
    db: Arc<DB>,
    parents_access: CachedDbAccess<Hash, Arc<Vec<Hash>>, BlockHasher>,
    children_store: DbChildrenStore,
}

impl DbRelationsStore {
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_size: u64) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let parents_prefix = DatabaseStorePrefixes::RelationsParents.into_iter().chain(lvl_bytes).collect_vec();

        Self {
            db: Arc::clone(&db),
            children_store: DbChildrenStore::new(db.clone(), level, cache_size),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_size: u64) -> Self {
        let parents_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsParents).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_store: DbChildrenStore::with_prefix(db, prefix, cache_size),
        }
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.parents_access.read(hash)
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        if !self.parents_access.has(hash)? {
            Err(StoreError::KeyNotFound(DbKey::new(self.parents_access.prefix(), hash)))
        } else {
            self.children_store.get(hash)
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.parents_access.has(hash)? {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self.parents_access.iterator().count();
        Ok((count, count))
    }
}

/// NOTE: we impl the trait on the store *reference* (and not over the store itself)
/// since its methods are defined as `&mut self` however callers do not need to pass an
/// actual `&mut store` since the Db store is thread-safe. By implementing on the reference
/// the caller can now pass `&mut &store` which is always available locally.
///
/// The trait methods itself must remain `&mut self` in order to support staging implementations
/// which are indeed mutated locally
impl ChildrenStore for &DbRelationsStore {
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        (&self.children_store).insert_child(writer, parent, child)
    }

    fn delete_children(&mut self, writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        (&self.children_store).delete_children(writer, parent)
    }

    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        (&self.children_store).delete_child(writer, parent, child)
    }
}

/// The comment above over `impl ChildrenStore` applies here as well
impl RelationsStore for &DbRelationsStore {
    type DefaultWriter = DirectDbWriter<'static>;

    fn default_writer(&self) -> Self::DefaultWriter {
        DirectDbWriter::from_arc(self.db.clone())
    }

    fn set_parents(&mut self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_access.write(writer, hash, parents)
    }

    fn delete_entries(&mut self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_access.delete(&mut writer, hash)?;
        (&self.children_store).delete_children(&mut writer, hash)
    }
}

#[derive(Default)]
struct StagingChildren {
    insertions: BlockHashMap<BlockHashSet>,
    deletions: BlockHashMap<BlockHashSet>,
    delete_all_children: BlockHashSet,
}

pub struct StagingRelationsStore<'a> {
    store: &'a DbRelationsStore,
    parents_insertions: BlockHashMap<BlockHashes>,
    parent_deletions: BlockHashSet,
    children: StagingChildren,
}

impl<'a> ChildrenStore for StagingRelationsStore<'a> {
    fn insert_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.check_not_in_children_delete_all(parent);
        match self.children.insertions.entry(parent) {
            Entry::Occupied(mut e) => {
                e.get_mut().insert(child);
            }
            Entry::Vacant(e) => {
                e.insert(HashSet::from_iter(once(child)));
            }
        };
        Ok(())
    }

    fn delete_children(&mut self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.children.delete_all_children.insert(parent);
        Ok(())
    }

    fn delete_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.check_not_in_children_delete_all(parent);
        match self.children.insertions.entry(parent) {
            Entry::Occupied(mut e) => {
                let removed = e.get_mut().remove(&child);
                if !removed {
                    match self.children.deletions.entry(parent) {
                        Entry::Occupied(mut e) => {
                            e.get_mut().insert(child);
                        }
                        Entry::Vacant(e) => {
                            e.insert(HashSet::from_iter(once(child)));
                        }
                    };
                }
            }
            Entry::Vacant(_) => {
                match self.children.deletions.entry(parent) {
                    Entry::Occupied(mut e) => {
                        e.get_mut().insert(child);
                    }
                    Entry::Vacant(e) => {
                        e.insert(HashSet::from_iter(once(child)));
                    }
                };
            }
        };

        Ok(())
    }
}

impl<'a> StagingRelationsStore<'a> {
    pub fn new(store: &'a DbRelationsStore) -> Self {
        Self { store, parents_insertions: Default::default(), parent_deletions: Default::default(), children: Default::default() }
    }

    pub fn commit(mut self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        for (k, v) in self.parents_insertions.iter() {
            self.store.parents_access.write(BatchDbWriter::new(batch), *k, (*v).clone())?
        }

        for (parent, children) in
            self.children.insertions.iter().filter(|(parent, _)| !self.children.delete_all_children.contains(parent))
        {
            for child in children.iter().copied() {
                self.store.insert_child(BatchDbWriter::new(batch), *parent, child)?;
            }
        }
        // Deletions always come after mutations
        self.store.parents_access.delete_many(BatchDbWriter::new(batch), &mut self.parent_deletions.iter().copied())?;
        for (parent, children_to_delete) in
            self.children.deletions.iter().filter(|(parent, _)| !self.children.delete_all_children.contains(parent))
        {
            for child in children_to_delete {
                self.store.delete_child(BatchDbWriter::new(batch), *parent, *child)?;
            }
        }

        for parent in self.children.delete_all_children.iter().copied() {
            self.store.delete_children(BatchDbWriter::new(batch), parent)?;
        }

        Ok(())
    }

    fn check_not_in_parent_deletions(&self, hash: Hash) -> Result<(), StoreError> {
        if self.parent_deletions.contains(&hash) {
            Err(StoreError::KeyNotFound(DbKey::new(b"staging-relations", hash)))
        } else {
            Ok(())
        }
    }

    fn check_not_in_children_delete_all(&self, parent: Hash) {
        if self.children.delete_all_children.contains(&parent) {
            panic!("{parent} children are already deleted")
        }
    }
}

impl RelationsStore for StagingRelationsStore<'_> {
    type DefaultWriter = MemoryWriter;

    fn default_writer(&self) -> Self::DefaultWriter {
        MemoryWriter
    }

    fn set_parents(&mut self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_insertions.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_insertions.remove(&hash);
        self.parent_deletions.insert(hash);
        self.delete_children(writer, hash)?;
        Ok(())
    }
}

impl RelationsStoreReader for StagingRelationsStore<'_> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_parent_deletions(hash)?;
        if let Some(data) = self.parents_insertions.get(&hash) {
            Ok(BlockHashes::clone(data))
        } else {
            self.store.get_parents(hash)
        }
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        self.check_not_in_parent_deletions(hash)?;
        let store_children = self.store.get_children(hash).unwrap_option().unwrap_or_default().read().iter().copied().collect_vec();
        if self.children.delete_all_children.contains(&hash) {
            return Ok(Default::default());
        }

        let insertions = self.children.insertions.get(&hash).cloned().unwrap_or_default();
        let deletions = self.children.deletions.get(&hash).cloned().unwrap_or_default();
        let children: BlockHashSet =
            BlockHashSet::from_iter(store_children.iter().copied().chain(insertions)).difference(&deletions).copied().collect();
        Ok(children.into())
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.parent_deletions.contains(&hash) {
            return Ok(false);
        }
        Ok(self.parents_insertions.contains_key(&hash) || self.store.has(hash)?)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self
            .store
            .parents_access
            .iterator()
            .map(|r| r.unwrap().0)
            .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
            .map(Hash::from_bytes)
            .chain(self.parents_insertions.keys().copied())
            .collect::<BlockHashSet>()
            .difference(&self.parent_deletions)
            .count();
        Ok((count, count))
    }
}

#[derive(Default)]
pub struct MemoryRelationsStore {
    parents_map: BlockHashMap<BlockHashes>,
    children_map: BlockHashMap<BlockHashes>,
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Default::default()
    }
}

impl ChildrenStore for MemoryRelationsStore {
    fn insert_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children = match self.children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        children.push(child);
        self.children_map.insert(parent, children.into());
        Ok(())
    }

    fn delete_children(&mut self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.children_map.remove(&parent);
        Ok(())
    }

    fn delete_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children = match self.children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        let Some((to_remove_idx, _)) = children.iter().find_position(|current| **current == child) else {
            return Ok(());
        };

        children.remove(to_remove_idx);
        self.children_map.insert(parent, children.into());
        Ok(())
    }
}

impl RelationsStoreReader for MemoryRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.parents_map.get(&hash) {
            Some(parents) => Ok(BlockHashes::clone(parents)),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsParents.as_ref(), hash))),
        }
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        if !self.has(hash)? {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsChildren.as_ref(), hash)))
        } else {
            match self.children_map.get(&hash) {
                Some(children) => Ok(BlockHashSet::from_iter(children.iter().copied()).into()),
                None => Ok(Default::default()),
            }
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.parents_map.contains_key(&hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self.parents_map.len();
        Ok((count, count))
    }
}

impl RelationsStore for MemoryRelationsStore {
    type DefaultWriter = MemoryWriter;

    fn default_writer(&self) -> Self::DefaultWriter {
        MemoryWriter
    }

    fn set_parents(&mut self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_map.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, _writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_map.remove(&hash);
        self.children_map.remove(&hash);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::relations::RelationsStoreExtensions;
    use kaspa_database::create_temp_db;

    #[test]
    fn test_memory_relations_store() {
        test_relations_store(MemoryRelationsStore::new());
    }

    #[test]
    fn test_db_relations_store() {
        let (lt, db) = create_temp_db!(kaspa_database::prelude::ConnBuilder::default().with_files_limit(10));
        test_relations_store(&DbRelationsStore::new(db, 0, 2));
        drop(lt)
    }

    fn test_relations_store<T: RelationsStore + ChildrenStore>(mut store: T) {
        let parents = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![2, 3]), (5, vec![1, 4])];
        for (i, vec) in parents.iter().cloned() {
            store.insert(i.into(), BlockHashes::new(vec.iter().copied().map(Hash::from).collect())).unwrap();
        }

        let expected_children = [(1, vec![2, 3, 5]), (2, vec![4]), (3, vec![4]), (4, vec![5]), (5, vec![])];
        for (i, vec) in expected_children {
            let store_children: BlockHashSet = store.get_children(i.into()).unwrap().read().iter().copied().collect();
            let expected: BlockHashSet = vec.iter().copied().map(Hash::from).collect();
            assert_eq!(store_children, expected);
        }

        for (i, vec) in parents {
            assert!(store.get_parents(i.into()).unwrap().iter().copied().eq(vec.iter().copied().map(Hash::from)));
        }
    }
}
