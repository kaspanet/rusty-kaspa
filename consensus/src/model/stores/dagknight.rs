use std::{cell::RefCell, collections::HashMap, sync::Arc};

use kaspa_consensus_core::KType;
use kaspa_database::{
    prelude::{DbKey, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;

use crate::model::stores::ghostdag::GhostdagData;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB};
use rocksdb::WriteBatch;

pub struct MemoryDagknightStore {
    dk_map: RefCell<HashMap<DagknightKey, Arc<GhostdagData>>>,
}

pub trait DagknightStoreReader {
    fn get_selected_parent(&self, dk_key: DagknightKey) -> Result<Hash, StoreError>;
    fn get_data(&self, dk_key: DagknightKey) -> Result<Arc<GhostdagData>, StoreError>;
    fn has(&self, dk_key: DagknightKey) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DagknightKey {
    pub pov_hash: Hash,
    pub root_hash: Hash,
    pub k: KType,
    pub free_search: bool,
    // Precomputed bytes in order: root_hash || pov_hash || k || free_search
    bytes: [u8; kaspa_hashes::HASH_SIZE * 2 + 2],
}

impl DagknightKey {
    pub fn new(root_hash: Hash, pov_hash: Hash, k: KType, free_search: bool) -> Self {
        let mut bytes = [0u8; kaspa_hashes::HASH_SIZE * 2 + 2];
        bytes[..kaspa_hashes::HASH_SIZE].copy_from_slice(root_hash.as_ref());
        bytes[kaspa_hashes::HASH_SIZE] = k as u8;
        bytes[(kaspa_hashes::HASH_SIZE + 1)..(2 * kaspa_hashes::HASH_SIZE + 1)].copy_from_slice(pov_hash.as_ref());
        bytes[(2 * kaspa_hashes::HASH_SIZE) + 1] = if free_search { 1 } else { 0 };

        Self { pov_hash, root_hash, k, free_search, bytes }
    }
}

impl ToString for DagknightKey {
    fn to_string(&self) -> String {
        format!("{:?}", &self.bytes)
    }
}

impl AsRef<[u8]> for DagknightKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Eq for DagknightKey {}

impl std::hash::Hash for DagknightKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash based on the logical key fields
        self.root_hash.hash(state);
        self.k.hash(state);
        self.pov_hash.hash(state);
        self.free_search.hash(state);
    }
}

impl PartialEq for DagknightKey {
    fn eq(&self, other: &Self) -> bool {
        self.pov_hash == other.pov_hash
            && self.k == other.k
            && self.root_hash == other.root_hash
            && self.free_search == other.free_search
    }
}

pub trait DagknightStore {
    fn insert(&self, key: DagknightKey, dk_data: Arc<GhostdagData>) -> Result<(), StoreError>;
    fn delete(&self, key: DagknightKey) -> Result<(), StoreError>;
    fn delete_rooted_range(&self, batch: &mut WriteBatch, hash: Hash) -> Result<u32, StoreError>;
}

impl MemoryDagknightStore {
    pub fn new(dk_map: RefCell<HashMap<DagknightKey, Arc<GhostdagData>>>) -> Self {
        Self { dk_map }
    }
}

impl DagknightStoreReader for MemoryDagknightStore {
    fn get_selected_parent(&self, dk_key: DagknightKey) -> Result<Hash, StoreError> {
        Ok(self.get_data(dk_key)?.selected_parent)
    }

    fn get_data(&self, key: DagknightKey) -> Result<Arc<GhostdagData>, StoreError> {
        if let Some(pov_block_dk_data) = self.dk_map.borrow().get(&key) {
            Ok(pov_block_dk_data.clone())
        } else {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::DagKnight.as_ref(), key)))
        }
    }

    fn has(&self, dk_key: DagknightKey) -> Result<bool, StoreError> {
        Ok(self.dk_map.borrow().contains_key(&dk_key))
    }
}

impl DagknightStore for MemoryDagknightStore {
    fn insert(&self, key: DagknightKey, dk_data: Arc<GhostdagData>) -> Result<(), StoreError> {
        self.dk_map.borrow_mut().insert(key, dk_data);

        Ok(())
    }

    fn delete(&self, key: DagknightKey) -> Result<(), StoreError> {
        self.dk_map.borrow_mut().remove(&key);

        Ok(())
    }

    fn delete_rooted_range(&self, _batch: &mut WriteBatch, _hash: Hash) -> Result<u32, StoreError> {
        unimplemented!()
    }
}

/// A DB + cache implementation of `DagknightStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDagknightStore {
    db: Arc<DB>,
    access: CachedDbAccess<DagknightKey, Arc<GhostdagData>>,
}

impl DbDagknightStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        let prefix = DatabaseStorePrefixes::DagKnight.as_ref().to_vec();
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, prefix) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, key: DagknightKey, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.access.has(key.clone())? {
            return Err(StoreError::KeyAlreadyExists(key.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), key, data)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, key: DagknightKey) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), key)
    }
}

impl DagknightStoreReader for DbDagknightStore {
    fn get_selected_parent(&self, dk_key: DagknightKey) -> Result<Hash, StoreError> {
        Ok(self.get_data(dk_key)?.selected_parent)
    }

    fn get_data(&self, dk_key: DagknightKey) -> Result<Arc<GhostdagData>, StoreError> {
        self.access.read(dk_key)
    }

    fn has(&self, dk_key: DagknightKey) -> Result<bool, StoreError> {
        self.access.has(dk_key)
    }
}

impl DagknightStore for DbDagknightStore {
    fn insert(&self, key: DagknightKey, dk_data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.access.has(key.clone())? {
            return Err(StoreError::KeyAlreadyExists(key.to_string()));
        }
        let mut batch = WriteBatch::default();
        self.access.write(BatchDbWriter::new(&mut batch), key, dk_data)?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete(&self, key: DagknightKey) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        self.access.delete(BatchDbWriter::new(&mut batch), key)?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete_rooted_range(&self, batch: &mut WriteBatch, hash: Hash) -> Result<u32, StoreError> {
        // delete records that have a prefix rooted at this DK store key + hash
        let root_bytes_prefix = {
            let mut bytes = Vec::with_capacity(kaspa_hashes::HASH_SIZE + 1);
            bytes.extend(DatabaseStorePrefixes::DagKnight.as_ref());
            bytes.extend_from_slice(hash.as_ref());
            bytes
        };
        let start_conflict_genesis_bytes = {
            let mut bytes = Vec::with_capacity(kaspa_hashes::HASH_SIZE + 2);
            bytes.extend_from_slice(&root_bytes_prefix);
            bytes.push(0); // k = 0 u16 first byte
            bytes.push(0); // k = 0 u16 second byte
            bytes
        };
        let end_conflict_genesis_bytes = {
            let mut bytes = Vec::with_capacity(kaspa_hashes::HASH_SIZE + 2);
            bytes.extend_from_slice(&root_bytes_prefix);
            // TODO[DK]: This range check misses entries where k = u16::MAX. However, we don't expect k to reach that value anyway
            // in practice so we don't expect records to exist here as well. In the DK implementation, k may be clamped to max out
            // lower than k = u16::MAX
            bytes.push(0xFF); // k = 0xFFFF u16 first byte
            bytes.push(0xFF); // k = 0xFFFF u16 second byte
            bytes
        };
        // TODO[DK]: count keys in range. Possibly would be removed.
        let mut count = 0;
        let mut iterator = self.db.raw_iterator();
        iterator.seek(&start_conflict_genesis_bytes);
        while iterator.valid() {
            let key = iterator.key();
            if key.unwrap() >= end_conflict_genesis_bytes.as_slice() {
                break;
            }
            count += 1;
            iterator.next();
        }
        // Perform the range delete
        batch.delete_range(start_conflict_genesis_bytes, end_conflict_genesis_bytes);
        Ok(count)
    }
}
