use itertools::Itertools;
use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::{blockhash::BlockHashes, BlockHashMap, BlockHasher, BlockLevel};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, DbWriter};
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
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_policy: CachePolicy, children_cache_policy: CachePolicy) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let parents_prefix = DatabaseStorePrefixes::RelationsParents.into_iter().chain(lvl_bytes).collect_vec();

        Self {
            db: Arc::clone(&db),
            children_store: DbChildrenStore::new(db.clone(), level, children_cache_policy),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_policy, parents_prefix),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_policy: CachePolicy, children_cache_policy: CachePolicy) -> Self {
        let parents_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsParents).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_policy, parents_prefix),
            children_store: DbChildrenStore::with_prefix(db, prefix, children_cache_policy),
        }
    }

    pub(crate) fn delete_children(&self, writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.children_store.delete_children(writer, parent)
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.parents_access.read(hash)
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        if !self.parents_access.has(hash)? {
            // Children store is iterator based so it might just be empty, hence we check
            // the parents store
            Err(StoreError::KeyNotFound(DbKey::new(self.children_store.prefix(), hash)))
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
    fn insert_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.children_store.insert_child(writer, parent, child)
    }

    fn delete_child(&mut self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.children_store.delete_child(writer, parent, child)
    }
}

impl RelationsStore for DbRelationsStore {
    type DefaultWriter = DirectDbWriter<'static>;

    fn default_writer(&self) -> Self::DefaultWriter {
        DirectDbWriter::from_arc(self.db.clone())
    }

    fn set_parents(&mut self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_access.write(writer, hash, parents)
    }

    fn delete_entries(&mut self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_access.delete(&mut writer, hash)?;
        self.children_store.delete_children(&mut writer, hash)
    }
}

pub struct StagingRelationsStore<'a> {
    // The underlying DB store to commit to
    store: &'a mut DbRelationsStore,

    /// Full entry deletions (including parents and all children)
    /// Assumed to be final, i.e., no other mutations to this entry
    /// are expected
    entry_deletions: BlockHashSet,

    /// Full parents list updates (inner set is not a diff but rather a full replacement)
    parents_overrides: BlockHashMap<BlockHashes>,

    /// Children insertions (inner set is a diff and reflects the new children to add)
    children_insertions: BlockHashMap<BlockHashSet>,

    /// Children deletions (inner set is a diff and reflects specific children to delete)
    children_deletions: BlockHashMap<BlockHashSet>,
}

impl ChildrenStore for StagingRelationsStore<'_> {
    fn insert_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.check_not_in_entry_deletions(parent)?;
        self.check_not_in_children_deletions(parent, child)?; // We expect deletion to be permanent
        match self.children_insertions.entry(parent) {
            Entry::Occupied(mut e) => {
                e.get_mut().insert(child);
            }
            Entry::Vacant(e) => {
                e.insert(HashSet::from_iter(once(child)));
            }
        };
        Ok(())
    }

    fn delete_child(&mut self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.check_not_in_entry_deletions(parent)?;
        if let Entry::Occupied(mut e) = self.children_insertions.entry(parent) {
            e.get_mut().remove(&child);
        };
        match self.children_deletions.entry(parent) {
            Entry::Occupied(mut e) => {
                e.get_mut().insert(child);
            }
            Entry::Vacant(e) => {
                e.insert(HashSet::from_iter(once(child)));
            }
        };

        Ok(())
    }
}

impl<'a> StagingRelationsStore<'a> {
    pub fn new(store: &'a mut DbRelationsStore) -> Self {
        Self {
            store,
            parents_overrides: Default::default(),
            entry_deletions: Default::default(),
            children_insertions: Default::default(),
            children_deletions: Default::default(),
        }
    }

    pub fn commit(&mut self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        for (k, v) in self.parents_overrides.iter() {
            self.store.parents_access.write(BatchDbWriter::new(batch), *k, (*v).clone())?
        }

        for (parent, children) in self.children_insertions.iter() {
            for child in children {
                self.store.insert_child(BatchDbWriter::new(batch), *parent, *child)?;
            }
        }

        //
        // Deletions always come after mutations
        //

        // For deleted entries, delete all parents
        self.store.parents_access.delete_many(BatchDbWriter::new(batch), &mut self.entry_deletions.iter().copied())?;

        // For deleted entries, delete all children
        for parent in self.entry_deletions.iter().copied() {
            self.store.delete_children(BatchDbWriter::new(batch), parent)?;
        }

        // Delete only the requested children
        for (parent, children_to_delete) in self.children_deletions.iter() {
            for child in children_to_delete {
                self.store.delete_child(BatchDbWriter::new(batch), *parent, *child)?;
            }
        }

        Ok(())
    }

    fn check_not_in_entry_deletions(&self, hash: Hash) -> Result<(), StoreError> {
        if self.entry_deletions.contains(&hash) {
            Err(StoreError::KeyNotFound(DbKey::new(b"staging-relations", hash)))
        } else {
            Ok(())
        }
    }

    fn check_not_in_children_deletions(&self, parent: Hash, child: Hash) -> Result<(), StoreError> {
        if let Some(e) = self.children_deletions.get(&parent) {
            if e.contains(&child) {
                Err(StoreError::KeyNotFound(DbKey::new_with_bucket(b"staging-relations", parent, child)))
            } else {
                Ok(())
            }
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

    fn set_parents(&mut self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_overrides.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, _writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_overrides.remove(&hash);
        self.children_deletions.remove(&hash);
        self.children_insertions.remove(&hash);
        self.entry_deletions.insert(hash);
        Ok(())
    }
}

impl RelationsStoreReader for StagingRelationsStore<'_> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_entry_deletions(hash)?;
        if let Some(data) = self.parents_overrides.get(&hash) {
            Ok(BlockHashes::clone(data))
        } else {
            self.store.get_parents(hash)
        }
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        self.check_not_in_entry_deletions(hash)?;
        let store_children = match self.store.get_children(hash) {
            Ok(c) => c.read().iter().copied().collect_vec(),
            // If both--store key not found and new insertions contain the key--then don't propagate the err.
            // We check parents as well since that is how the underlying store verifies children key existence
            Err(StoreError::KeyNotFound(_))
                if self.parents_overrides.contains_key(&hash) || self.children_insertions.contains_key(&hash) =>
            {
                Vec::new()
            }
            Err(err) => return Err(err),
        };
        let insertions = self.children_insertions.get(&hash).cloned().unwrap_or_default();
        let deletions = self.children_deletions.get(&hash).cloned().unwrap_or_default();
        let children: BlockHashSet =
            BlockHashSet::from_iter(store_children.iter().copied().chain(insertions)).difference(&deletions).copied().collect();
        Ok(children.into())
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.entry_deletions.contains(&hash) {
            return Ok(false);
        }
        Ok(self.parents_overrides.contains_key(&hash) || self.store.has(hash)?)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self
            .store
            .parents_access
            .iterator()
            .map(|r| r.unwrap().0)
            .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
            .map(Hash::from_bytes)
            .chain(self.parents_overrides.keys().copied())
            .collect::<BlockHashSet>()
            .difference(&self.entry_deletions)
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
    use kaspa_utils::mem_size::MemMode;

    #[test]
    fn test_memory_relations_store() {
        test_relations_store(MemoryRelationsStore::new());
    }

    #[test]
    fn test_db_relations_store() {
        let (lt, db) = create_temp_db!(kaspa_database::prelude::ConnBuilder::default().with_files_limit(10)).unwrap();
        test_relations_store(DbRelationsStore::new(
            db,
            0,
            CachePolicy::Tracked { max_size: 2, min_items: 0, mem_mode: MemMode::Units },
            CachePolicy::Tracked { max_size: 2, min_items: 0, mem_mode: MemMode::Units },
        ));
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
