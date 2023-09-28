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
use rocksdb::WriteBatch;
use std::sync::Arc;

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
    fn set_children(&mut self, writer: impl DbWriter, hash: Hash, children: BlockHashes) -> Result<(), StoreError>;
    fn delete_entries(&mut self, writer: impl DbWriter, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `RelationsStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbRelationsStore {
    db: Arc<DB>,
    parents_access: CachedDbAccess<Hash, Arc<Vec<Hash>>, BlockHasher>,
    children_access: CachedDbAccess<Hash, Arc<Vec<Hash>>, BlockHasher>,
}

impl DbRelationsStore {
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_size: u64) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let parents_prefix = DatabaseStorePrefixes::RelationsParents.into_iter().chain(lvl_bytes).collect_vec();
        let children_prefix = DatabaseStorePrefixes::RelationsChildren.into_iter().chain(lvl_bytes).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_access: CachedDbAccess::new(db, cache_size, children_prefix),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8], cache_size: u64) -> Self {
        let parents_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsParents).collect_vec();
        let children_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsChildren).collect_vec();
        Self {
            db: Arc::clone(&db),
            parents_access: CachedDbAccess::new(Arc::clone(&db), cache_size, parents_prefix),
            children_access: CachedDbAccess::new(db, cache_size, children_prefix),
        }
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
    type DefaultWriter = DirectDbWriter<'static>;

    fn default_writer(&self) -> Self::DefaultWriter {
        DirectDbWriter::from_arc(self.db.clone())
    }

    fn set_parents(&mut self, writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_access.write(writer, hash, parents)
    }

    fn set_children(&mut self, writer: impl DbWriter, hash: Hash, children: BlockHashes) -> Result<(), StoreError> {
        self.children_access.write(writer, hash, children)
    }

    fn delete_entries(&mut self, mut writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.parents_access.delete(&mut writer, hash)?;
        self.children_access.delete(&mut writer, hash)
    }
}

pub struct StagingRelationsStore<'a> {
    store: &'a mut DbRelationsStore,
    staging_parents_writes: BlockHashMap<BlockHashes>,
    staging_children_writes: BlockHashMap<BlockHashes>,
    staging_deletions: BlockHashSet,
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
        for (k, v) in self.staging_children_writes {
            self.store.children_access.write(BatchDbWriter::new(batch), k, v)?
        }
        // Deletions always come after mutations
        self.store.parents_access.delete_many(BatchDbWriter::new(batch), &mut self.staging_deletions.iter().copied())?;
        self.store.children_access.delete_many(BatchDbWriter::new(batch), &mut self.staging_deletions.iter().copied())
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

    fn set_children(&mut self, _writer: impl DbWriter, hash: Hash, children: BlockHashes) -> Result<(), StoreError> {
        self.staging_children_writes.insert(hash, children);
        Ok(())
    }

    fn delete_entries(&mut self, _writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.staging_parents_writes.remove(&hash);
        self.staging_children_writes.remove(&hash);
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
        if let Some(data) = self.staging_children_writes.get(&hash) {
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
        Ok((
            self.store
                .parents_access
                .iterator()
                .map(|r| r.unwrap().0)
                .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
                .map(Hash::from_bytes)
                .chain(self.staging_parents_writes.keys().copied())
                .collect::<BlockHashSet>()
                .difference(&self.staging_deletions)
                .count(),
            self.store
                .children_access
                .iterator()
                .map(|r| r.unwrap().0)
                .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
                .map(Hash::from_bytes)
                .chain(self.staging_children_writes.keys().copied())
                .collect::<BlockHashSet>()
                .difference(&self.staging_deletions)
                .count(),
        ))
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
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsParents.as_ref(), hash))),
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.children_map.get(&hash) {
            Some(children) => Ok(BlockHashes::clone(children)),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::RelationsChildren.as_ref(), hash))),
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
    type DefaultWriter = MemoryWriter;

    fn default_writer(&self) -> Self::DefaultWriter {
        MemoryWriter
    }

    fn set_parents(&mut self, _writer: impl DbWriter, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.parents_map.insert(hash, parents);
        Ok(())
    }

    fn set_children(&mut self, _writer: impl DbWriter, hash: Hash, children: BlockHashes) -> Result<(), StoreError> {
        self.children_map.insert(hash, children);
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
        test_relations_store(DbRelationsStore::new(db, 0, 2));
        drop(lt)
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
