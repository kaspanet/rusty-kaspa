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
use std::cell::RefCell;
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

/// Low-level write API for `RelationsStore`
pub trait RelationsStore: RelationsStoreReader {
    type DefaultWriter: DirectWriter;
    fn default_writer(&self) -> Self::DefaultWriter;

    fn set_parents(&self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError>;
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

impl ChildrenStore for DbRelationsStore {
    fn insert_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.children_store.insert_child(writer, parent, child)
    }

    fn delete_children(&self, writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.children_store.delete_children(writer, parent)
    }

    fn delete_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.children_store.delete_child(writer, parent, child)
    }
}

impl RelationsStore for DbRelationsStore {
    type DefaultWriter = DirectDbWriter<'static>;

    fn default_writer(&self) -> Self::DefaultWriter {
        DirectDbWriter::from_arc(self.db.clone())
    }

    fn set_parents(&self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_access.write(writer, hash, parents)
    }

    fn delete_entries(&mut self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_access.delete(&mut writer, hash)?;
        self.children_store.delete_children(&mut writer, hash)
    }
}

#[derive(Default)]
struct StagingChildren {
    insertions: BlockHashMap<BlockHashSet>,
    deletions: BlockHashMap<BlockHashSet>,
}
#[derive(Default)]
struct Staging {
    parents_insertions: BlockHashMap<BlockHashes>,
    parent_deletions: BlockHashSet,
    children: StagingChildren,
}

pub struct StagingRelationsStore<'a> {
    store: &'a DbRelationsStore,
    staging: RefCell<Staging>, // Because ChildrenStore and RelationsStore doesn't use `&mut self` in its methods,
                               // StagingRelationsStore cannot implement it while using `&mut self`, so we need
                               // to wrap `Staging` with RefCell. This is safe because  `StagingRelationsStore`
                               // doesn't have multiple mutable references anywhere it's used.
}

impl<'a> ChildrenStore for StagingRelationsStore<'a> {
    fn insert_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.staging.borrow_mut();
        match write_guard.children.insertions.entry(parent) {
            Entry::Occupied(mut e) => {
                e.get_mut().insert(child);
            }
            Entry::Vacant(e) => {
                e.insert(HashSet::from_iter(once(child)));
            }
        };
        Ok(())
    }

    fn delete_children(&self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.staging.borrow_mut();
        write_guard.children.insertions.remove(&parent);
        let store_children =
            self.store.children_store.get(parent).unwrap_option().unwrap_or_default().read().iter().copied().collect_vec();

        for child in store_children {
            match write_guard.children.deletions.entry(parent) {
                Entry::Occupied(mut e) => {
                    e.get_mut().insert(child);
                }
                Entry::Vacant(e) => {
                    e.insert(HashSet::from_iter(once(child)));
                }
            };
        }
        Ok(())
    }

    fn delete_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.staging.borrow_mut();
        match write_guard.children.insertions.entry(parent) {
            Entry::Occupied(mut e) => {
                let removed = e.get_mut().remove(&child);
                if !removed {
                    match write_guard.children.deletions.entry(parent) {
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
                match write_guard.children.deletions.entry(parent) {
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
        Self { store, staging: Default::default() }
    }

    pub fn commit(self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        let read_guard = self.staging.borrow();
        for (k, v) in read_guard.parents_insertions.iter() {
            self.store.parents_access.write(BatchDbWriter::new(batch), *k, (*v).clone())?
        }

        for (parent, children) in read_guard.children.insertions.iter() {
            for child in children.iter().copied() {
                self.store.children_store.insert_child(BatchDbWriter::new(batch), *parent, child)?;
            }
        }
        // Deletions always come after mutations
        self.store.parents_access.delete_many(BatchDbWriter::new(batch), &mut read_guard.parent_deletions.iter().copied())?;
        for (parent, children_to_delete) in read_guard.children.deletions.iter() {
            for child in children_to_delete {
                self.store.delete_child(BatchDbWriter::new(batch), *parent, *child)?;
            }
        }

        Ok(())
    }

    fn check_not_in_deletions(&self, hash: Hash) -> Result<(), StoreError> {
        if self.staging.borrow().parent_deletions.contains(&hash) {
            Err(StoreError::KeyNotFound(DbKey::new(b"staging-relations", hash)))
        } else {
            Ok(())
        }
    }
}

impl RelationsStore for StagingRelationsStore<'_> {
    type DefaultWriter = MemoryWriter;

    fn default_writer(&self) -> Self::DefaultWriter {
        MemoryWriter
    }

    fn set_parents(&self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.staging.borrow_mut().parents_insertions.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.staging.borrow_mut();
        write_guard.parents_insertions.remove(&hash);
        write_guard.parent_deletions.insert(hash);
        drop(write_guard);
        self.delete_children(writer, hash)?;
        Ok(())
    }
}

impl RelationsStoreReader for StagingRelationsStore<'_> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging.borrow().parents_insertions.get(&hash) {
            Ok(BlockHashes::clone(data))
        } else {
            self.store.get_parents(hash)
        }
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        self.check_not_in_deletions(hash)?;
        let store_children = self.store.get_children(hash).unwrap_option().unwrap_or_default().read().iter().copied().collect_vec();
        let read_guard = self.staging.borrow();
        let insertions = read_guard.children.insertions.get(&hash).cloned().unwrap_or_default();
        let deletions = read_guard.children.deletions.get(&hash).cloned().unwrap_or_default();
        let children: BlockHashSet =
            BlockHashSet::from_iter(store_children.iter().copied().chain(insertions)).difference(&deletions).copied().collect();
        Ok(children.into())
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        let read_guard = self.staging.borrow();
        if read_guard.parent_deletions.contains(&hash) {
            return Ok(false);
        }
        Ok(read_guard.parents_insertions.contains_key(&hash) || self.store.has(hash)?)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let read_guard = self.staging.borrow();
        let count = self
            .store
            .parents_access
            .iterator()
            .map(|r| r.unwrap().0)
            .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
            .map(Hash::from_bytes)
            .chain(read_guard.parents_insertions.keys().copied())
            .collect::<BlockHashSet>()
            .difference(&read_guard.parent_deletions)
            .count();
        Ok((count, count))
    }
}

pub struct MemoryRelationsStore(RefCell<MemoryRelationsStoreInner>); // Because ChildrenStore and RelationsStore doesn't use `&mut self` in its methods,
                                                                     // MemoryRelationsStore cannot implement it while using `&mut self`, so we need
                                                                     // to wrap `MemoryRelationsStoreInner` with RefCell. This is safe because  `MemoryRelationsStore`
                                                                     // doesn't have multiple mutable references anywhere it's used.

#[derive(Default)]
struct MemoryRelationsStoreInner {
    parents_map: BlockHashMap<BlockHashes>,
    children_map: BlockHashMap<BlockHashes>,
}

impl ChildrenStore for MemoryRelationsStore {
    fn insert_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.0.borrow_mut();
        let mut children = match write_guard.children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        children.push(child);
        write_guard.children_map.insert(parent, children.into());
        Ok(())
    }

    fn delete_children(&self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.0.borrow_mut().children_map.remove(&parent);
        Ok(())
    }

    fn delete_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.0.borrow_mut();
        let mut children = match write_guard.children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        let Some((to_remove_idx, _)) = children.iter().find_position(|current| **current == child) else {
            return Ok(());
        };

        children.remove(to_remove_idx);
        write_guard.children_map.insert(parent, children.into());
        Ok(())
    }
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Self(Default::default())
    }
}

impl Default for MemoryRelationsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationsStoreReader for MemoryRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.0.borrow().parents_map.get(&hash) {
            Some(parents) => Ok(BlockHashes::clone(parents)),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsParents.as_ref(), hash))),
        }
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        if !self.has(hash)? {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsChildren.as_ref(), hash)))
        } else {
            match self.0.borrow().children_map.get(&hash) {
                Some(children) => Ok(BlockHashSet::from_iter(children.iter().copied()).into()),
                None => Ok(Default::default()),
            }
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.0.borrow().parents_map.contains_key(&hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self.0.borrow().parents_map.len();
        Ok((count, count))
    }
}

impl RelationsStore for MemoryRelationsStore {
    type DefaultWriter = MemoryWriter;

    fn default_writer(&self) -> Self::DefaultWriter {
        MemoryWriter
    }

    fn set_parents(&self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.0.borrow_mut().parents_map.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, _writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        let mut write_guard = self.0.borrow_mut();
        write_guard.parents_map.remove(&hash);
        write_guard.children_map.remove(&hash);
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
        test_relations_store(DbRelationsStore::new(db, 0, 2));
        drop(lt)
    }

    fn test_relations_store<T: RelationsStore + ChildrenStore>(store: T) {
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
