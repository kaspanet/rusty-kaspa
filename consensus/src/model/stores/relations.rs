use itertools::Itertools;

use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::{blockhash::BlockHashes, BlockHashMap, BlockHasher, BlockLevel, HashMapCustomHasher};
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, DbWriter};
use kaspa_database::prelude::{CachedDbAccess, DbKey, DirectDbWriter};
use kaspa_database::prelude::{DirectWriter, MemoryWriter};
use kaspa_database::registry::{DatabaseStorePrefixes, SEPARATOR};
use kaspa_hashes::Hash;
use parking_lot::RwLock;
use rocksdb::WriteBatch;
use std::sync::Arc;

use super::children::{ChildrenStore, ChildrenStoreReader, DbChildrenStore};

/// Reader API for `RelationsStore`.
pub trait RelationsStoreReader {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
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
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_size: u64) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let parents_prefix = DatabaseStorePrefixes::RelationsParents.into_iter().chain(lvl_bytes).collect_vec();

        Self {
            db: Arc::clone(&db),
            children_store: DbChildrenStore::new(db.clone(), level),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_size: u64) -> Self {
        let parents_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsParents).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_store: DbChildrenStore::with_prefix(db, prefix),
        }
    }
}

impl RelationsStoreReader for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.parents_access.read(hash)
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        if !self.parents_access.has(hash)? {
            Err(StoreError::KeyNotFound(DbKey::new(self.parents_access.prefix(), hash)))
        } else {
            Ok(self.children_store.get(hash).unwrap().into())
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

    fn set_parents(&mut self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_access.write(writer, hash, parents)
    }

    fn delete_entries(&mut self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_access.delete(&mut writer, hash)?;
        self.children_store.delete_children(&mut writer, hash)
    }
}

pub struct StagingRelationsStore<'a> {
    store: &'a mut DbRelationsStore,
    staging_parents_writes: BlockHashMap<BlockHashes>,
    staging_children_writes: RwLock<BlockHashMap<BlockHashes>>,
    staging_deletions: BlockHashSet,
}

impl<'a> ChildrenStore for StagingRelationsStore<'a> {
    fn insert_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children = {
            self.check_not_in_deletions(parent)?;
            if let Some(data) = self.staging_children_writes.read().get(&parent) {
                data.iter().copied().collect_vec()
            } else {
                self.store.children_store.get(parent).unwrap()
            }
        };

        children.push(child);
        self.staging_children_writes.write().insert(parent, children.into());
        Ok(())
    }

    fn delete_children(&self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.staging_children_writes.write().insert(parent, Default::default());
        Ok(())
    }

    fn delete_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children = {
            self.check_not_in_deletions(parent)?;
            if let Some(data) = self.staging_children_writes.read().get(&parent) {
                Ok(BlockHashes::clone(data))
            } else {
                self.store.get_children(parent)
            }
        }?
        .iter()
        .copied()
        .collect_vec(); // TODO: Use self.get_children

        let Some((to_remove_idx, _)) = children.iter().find_position(|current| **current == child) else {
            return Ok(());
        };

        children.remove(to_remove_idx);
        self.staging_children_writes.write().insert(parent, children.into());
        Ok(())
    }
}

impl<'a> StagingRelationsStore<'a> {
    pub fn new(store: &'a mut DbRelationsStore) -> Self {
        Self {
            store,
            staging_parents_writes: Default::default(),
            staging_children_writes: Default::default(),
            staging_deletions: Default::default(),
        }
    }

    pub fn commit(self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        for (k, v) in self.staging_parents_writes {
            self.store.parents_access.write(BatchDbWriter::new(batch), k, v)?
        }
        for (k, v) in self.staging_children_writes.read().iter() {
            let store_children = BlockHashSet::from_iter(self.store.children_store.get(*k).unwrap().iter().copied());
            let new_children = BlockHashSet::from_iter(v.iter().copied());
            let children_to_delete = store_children.difference(&new_children);
            let children_to_add = new_children.difference(&store_children);

            for child in children_to_delete {
                self.store.children_store.delete_child(BatchDbWriter::new(batch), *k, *child)?;
            }

            for child in children_to_add {
                self.store.children_store.insert_child(BatchDbWriter::new(batch), *k, *child)?;
            }
            // TODO: Optimize it
        }
        // Deletions always come after mutations
        self.store.parents_access.delete_many(BatchDbWriter::new(batch), &mut self.staging_deletions.iter().copied())?;
        for k in self.staging_deletions {
            self.store.children_store.delete_children(BatchDbWriter::new(batch), k)?;
        }

        Ok(())
    }

    fn check_not_in_deletions(&self, hash: Hash) -> Result<(), StoreError> {
        if self.staging_deletions.contains(&hash) {
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

    fn set_parents(&mut self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.staging_parents_writes.insert(hash, parents);
        Ok(())
    }

    fn delete_entries(&mut self, _writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.staging_parents_writes.remove(&hash);
        self.staging_children_writes.write().remove(&hash);
        self.staging_deletions.insert(hash);
        Ok(())
    }
}

impl RelationsStoreReader for StagingRelationsStore<'_> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_parents_writes.get(&hash) {
            Ok(BlockHashes::clone(data))
        } else {
            self.store.get_parents(hash)
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_children_writes.read().get(&hash) {
            Ok(BlockHashes::clone(data))
        } else {
            self.store.get_children(hash)
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.staging_deletions.contains(&hash) {
            return Ok(false);
        }
        Ok(self.staging_parents_writes.contains_key(&hash) || self.store.has(hash)?)
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        let count = self
            .store
            .parents_access
            .iterator()
            .map(|r| r.unwrap().0)
            .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
            .map(Hash::from_bytes)
            .chain(self.staging_parents_writes.keys().copied())
            .collect::<BlockHashSet>()
            .difference(&self.staging_deletions)
            .count();
        Ok((count, count))
    }
}

pub struct MemoryRelationsStore {
    parents_map: BlockHashMap<BlockHashes>,
    children_map: RwLock<BlockHashMap<BlockHashes>>,
}

impl ChildrenStore for MemoryRelationsStore {
    fn insert_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children_map = self.children_map.write();
        let mut children = match children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        children.push(child);
        children_map.insert(parent, children.into());
        Ok(())
    }

    fn delete_children(&self, _writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        self.children_map.write().remove(&parent);
        Ok(())
    }

    fn delete_child(&self, _writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let mut children_map = self.children_map.write();
        let mut children = match children_map.get(&parent) {
            Some(children) => children.iter().copied().collect_vec(),
            None => vec![],
        };

        let Some((to_remove_idx, _)) = children.iter().find_position(|current| **current == child) else {
            return Ok(());
        };

        children.remove(to_remove_idx);
        children_map.insert(parent, children.into());
        Ok(())
    }
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Self { parents_map: BlockHashMap::new(), children_map: RwLock::new(BlockHashMap::new()) }
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
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsParents.as_ref(), hash))),
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        if !self.has(hash)? {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsChildren.as_ref(), hash)))
        } else {
            match self.children_map.read().get(&hash) {
                Some(children) => Ok(BlockHashes::clone(children)),
                None => Ok(Default::default()),
            }
        }
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.parents_map.contains_key(&hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        Ok((self.parents_map.len(), self.parents_map.len()))
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
        self.children_map.write().remove(&hash);
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

    fn test_relations_store<T: RelationsStore + ChildrenStore>(mut store: T) {
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
