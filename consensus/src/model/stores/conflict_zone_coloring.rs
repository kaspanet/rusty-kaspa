//! Conflict zone coloring store: per-block k-coloring data of a
//! conflict zone, keyed by (conflict genesis, k, pov, free search).

use std::{cell::RefCell, collections::HashMap, fmt, sync::Arc};

use kaspa_consensus_core::{BlockHashMap, BlueWorkType, HashKTypeMap, HashMapCustomHasher, KType, blockhash::BlockHashes};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB};
use kaspa_database::{
    prelude::{DbKey, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

/// The per-block POV coloring
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct ColoringData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: Hash,
    pub mergeset_blues: BlockHashes,
    pub mergeset_reds: BlockHashes,
    pub blues_anticone_sizes: HashKTypeMap,
}

impl ColoringData {
    pub fn new(
        blue_score: u64,
        blue_work: BlueWorkType,
        selected_parent: Hash,
        mergeset_blues: BlockHashes,
        mergeset_reds: BlockHashes,
        blues_anticone_sizes: HashKTypeMap,
    ) -> Self {
        Self { blue_score, blue_work, selected_parent, mergeset_blues, mergeset_reds, blues_anticone_sizes }
    }

    pub fn new_with_selected_parent(selected_parent: Hash, k: KType) -> Self {
        let mut mergeset_blues: Vec<Hash> = Vec::with_capacity((k + 1) as usize);
        let mut blues_anticone_sizes: BlockHashMap<KType> = BlockHashMap::with_capacity(k as usize);
        mergeset_blues.push(selected_parent);
        blues_anticone_sizes.insert(selected_parent, 0);

        Self {
            blue_score: Default::default(),
            blue_work: Default::default(),
            selected_parent,
            mergeset_blues: BlockHashes::new(mergeset_blues),
            mergeset_reds: Default::default(),
            blues_anticone_sizes: HashKTypeMap::new(blues_anticone_sizes),
        }
    }

    pub fn add_blue(&mut self, block: Hash, blue_anticone_size: KType, block_blues_anticone_sizes: &BlockHashMap<KType>) {
        // Add the new blue block to mergeset blues
        BlockHashes::make_mut(&mut self.mergeset_blues).push(block);

        // Get a mut ref to internal anticone size map
        let blues_anticone_sizes = HashKTypeMap::make_mut(&mut self.blues_anticone_sizes);

        // Insert the new blue block with its blue anticone size to the map
        blues_anticone_sizes.insert(block, blue_anticone_size);

        // Insert/update map entries for blocks affected by this insertion
        for (blue, size) in block_blues_anticone_sizes {
            blues_anticone_sizes.insert(*blue, size + 1);
        }
    }

    pub fn add_red(&mut self, block: Hash) {
        // Add the new red block to mergeset reds
        BlockHashes::make_mut(&mut self.mergeset_reds).push(block);
    }

    pub fn finalize_coloring_data(&mut self, blue_score: u64, blue_work: BlueWorkType) {
        self.blue_score = blue_score;
        self.blue_work = blue_work;
    }
}

impl MemSizeEstimator for ColoringData {
    fn estimate_mem_bytes(&self) -> usize {
        let mut bytes = size_of::<Self>();
        bytes += (self.mergeset_blues.len() + self.mergeset_reds.len()) * size_of::<Hash>();
        bytes += self.blues_anticone_sizes.len() * size_of::<(Hash, KType)>();
        bytes
    }
}

pub struct MemoryConflictZoneColoringStore {
    coloring_map: RefCell<HashMap<ConflictZoneColoringKey, Arc<ColoringData>>>,
}

pub trait ConflictZoneColoringStoreReader {
    fn get_selected_parent(&self, key: ConflictZoneColoringKey) -> Result<Hash, StoreError>;
    fn get_data(&self, key: ConflictZoneColoringKey) -> Result<Arc<ColoringData>, StoreError>;
    fn has(&self, key: ConflictZoneColoringKey) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct ConflictZoneColoringKey {
    pub pov_hash: Hash,
    pub root_hash: Hash,
    pub k: KType,
    pub free_search: bool,
    // Precomputed bytes in order: root_hash || k(u16 BE) || pov_hash || free_search
    bytes: [u8; kaspa_hashes::HASH_SIZE * 2 + 3],
}

impl ConflictZoneColoringKey {
    pub fn new(root_hash: Hash, pov_hash: Hash, k: KType, free_search: bool) -> Self {
        // Layout must match DB-level expectations where `k` is encoded as a u16
        // (two bytes). Allocate enough space: root_hash + k(2) + pov_hash + free_search(1).
        let mut bytes = [0u8; kaspa_hashes::HASH_SIZE * 2 + 3];
        let hash_size = kaspa_hashes::HASH_SIZE;
        bytes[..hash_size].copy_from_slice(root_hash.as_ref());

        // Encode k as big-endian u16 to match other code paths that construct
        // DB keys using two bytes for k.
        let k_be = k.to_be_bytes();
        bytes[hash_size] = k_be[0];
        bytes[hash_size + 1] = k_be[1];

        bytes[(hash_size + 2)..(hash_size + 2 + hash_size)].copy_from_slice(pov_hash.as_ref());
        bytes[(2 * hash_size) + 2] = if free_search { 1 } else { 0 };

        Self { pov_hash, root_hash, k, free_search, bytes }
    }
}

impl fmt::Display for ConflictZoneColoringKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.bytes)
    }
}

impl AsRef<[u8]> for ConflictZoneColoringKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Eq for ConflictZoneColoringKey {}

impl std::hash::Hash for ConflictZoneColoringKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash based on the logical key fields
        self.root_hash.hash(state);
        self.k.hash(state);
        self.pov_hash.hash(state);
        self.free_search.hash(state);
    }
}

impl PartialEq for ConflictZoneColoringKey {
    fn eq(&self, other: &Self) -> bool {
        self.pov_hash == other.pov_hash
            && self.k == other.k
            && self.root_hash == other.root_hash
            && self.free_search == other.free_search
    }
}

pub trait ConflictZoneColoringStore {
    fn insert(&self, key: ConflictZoneColoringKey, data: Arc<ColoringData>) -> Result<(), StoreError>;
    fn delete(&self, key: ConflictZoneColoringKey) -> Result<(), StoreError>;
    fn delete_rooted_range(&self, batch: &mut WriteBatch, hash: Hash) -> Result<u32, StoreError>;
}

impl MemoryConflictZoneColoringStore {
    pub fn new(coloring_map: RefCell<HashMap<ConflictZoneColoringKey, Arc<ColoringData>>>) -> Self {
        Self { coloring_map }
    }
}

impl ConflictZoneColoringStoreReader for MemoryConflictZoneColoringStore {
    fn get_selected_parent(&self, key: ConflictZoneColoringKey) -> Result<Hash, StoreError> {
        Ok(self.get_data(key)?.selected_parent)
    }

    fn get_data(&self, key: ConflictZoneColoringKey) -> Result<Arc<ColoringData>, StoreError> {
        if let Some(pov_block_coloring) = self.coloring_map.borrow().get(&key) {
            Ok(pov_block_coloring.clone())
        } else {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::DagKnight.as_ref(), key)))
        }
    }

    fn has(&self, key: ConflictZoneColoringKey) -> Result<bool, StoreError> {
        Ok(self.coloring_map.borrow().contains_key(&key))
    }
}

impl ConflictZoneColoringStore for MemoryConflictZoneColoringStore {
    fn insert(&self, key: ConflictZoneColoringKey, data: Arc<ColoringData>) -> Result<(), StoreError> {
        self.coloring_map.borrow_mut().insert(key, data);

        Ok(())
    }

    fn delete(&self, key: ConflictZoneColoringKey) -> Result<(), StoreError> {
        self.coloring_map.borrow_mut().remove(&key);

        Ok(())
    }

    fn delete_rooted_range(&self, _batch: &mut WriteBatch, _hash: Hash) -> Result<u32, StoreError> {
        unimplemented!()
    }
}

/// A DB + cache implementation of `ConflictZoneColoringStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbConflictZoneColoringStore {
    db: Arc<DB>,
    access: CachedDbAccess<ConflictZoneColoringKey, Arc<ColoringData>>,
}

impl DbConflictZoneColoringStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        // The DB prefix (80) and key layout are unchanged; only the value type
        // (and its layout) changed in place.
        let prefix = DatabaseStorePrefixes::DagKnight.as_ref().to_vec();
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, prefix) }
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        key: ConflictZoneColoringKey,
        data: Arc<ColoringData>,
    ) -> Result<(), StoreError> {
        if self.access.has(key.clone())? {
            return Err(StoreError::KeyAlreadyExists(key.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), key, data)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, key: ConflictZoneColoringKey) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), key)
    }
}

impl ConflictZoneColoringStoreReader for DbConflictZoneColoringStore {
    fn get_selected_parent(&self, key: ConflictZoneColoringKey) -> Result<Hash, StoreError> {
        Ok(self.get_data(key)?.selected_parent)
    }

    fn get_data(&self, key: ConflictZoneColoringKey) -> Result<Arc<ColoringData>, StoreError> {
        self.access.read(key)
    }

    fn has(&self, key: ConflictZoneColoringKey) -> Result<bool, StoreError> {
        self.access.has(key)
    }
}

impl ConflictZoneColoringStore for DbConflictZoneColoringStore {
    fn insert(&self, key: ConflictZoneColoringKey, data: Arc<ColoringData>) -> Result<(), StoreError> {
        if self.access.has(key.clone())? {
            return Err(StoreError::KeyAlreadyExists(key.to_string()));
        }
        let mut batch = WriteBatch::default();
        self.access.write(BatchDbWriter::new(&mut batch), key, data)?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete(&self, key: ConflictZoneColoringKey) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        self.access.delete(BatchDbWriter::new(&mut batch), key)?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete_rooted_range(&self, batch: &mut WriteBatch, hash: Hash) -> Result<u32, StoreError> {
        // delete records that have a prefix rooted at this coloring store key + hash
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dagknight_key_encodes_free_search_flag() {
        use kaspa_hashes::Hash;

        let root: Hash = 0xAA_u64.into();
        let pov: Hash = 0xBB_u64.into();
        let k: KType = 5;

        let key_committed = ConflictZoneColoringKey::new(root, pov, k, false);
        let key_free = ConflictZoneColoringKey::new(root, pov, k, true);

        // Keys with different free_search values must be distinct
        assert_ne!(key_committed.as_ref(), key_free.as_ref());

        // Verify the free_search flag is encoded as the last byte
        assert_eq!(key_committed.as_ref().last().unwrap(), &0u8);
        assert_eq!(key_free.as_ref().last().unwrap(), &1u8);
    }

    #[test]
    fn test_coloring_key_encodes_k() {
        let root: Hash = 0xAA_u64.into();
        let pov: Hash = 0xBB_u64.into();
        let k1: KType = 0x0001;
        let k2: KType = 0x0101; // 2nd byte is the same as above

        let key1 = ConflictZoneColoringKey::new(root, pov, k1, false);
        let key2 = ConflictZoneColoringKey::new(root, pov, k2, false);

        // The DB key bytes must differ when k differs. This captures the previous
        // bug where `k` was encoded incorrectly and keys collided across k values.
        println!("key1 bytes: {:?}", key1.as_ref());
        println!("key2 bytes: {:?}", key2.as_ref());
        assert_ne!(key1.as_ref(), key2.as_ref(), "ConflictZoneColoringKey DB bytes must encode k uniquely");

        // Also assert that the differing two-byte `k` slot differs (sanity check on layout)
        let hash_size = kaspa_hashes::HASH_SIZE;
        // k is encoded as two bytes after root_hash
        assert_ne!(
            &key1.as_ref()[hash_size..hash_size + 2],
            &key2.as_ref()[hash_size..hash_size + 2],
            "k slot (two bytes) must differ for different k values"
        );
    }

    #[test]
    fn test_db_coloring_store_isolates_by_k() {
        use kaspa_database::prelude::CachePolicy;
        use kaspa_database::prelude::ConnBuilder;
        use std::sync::Arc;

        // Create a temporary RocksDB
        let (_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let store = DbConflictZoneColoringStore::new(db.clone(), CachePolicy::Count(16));

        let root: Hash = 0xAA_u64.into();
        let pov: Hash = 0xBB_u64.into();

        let k1 = 0x0001;
        let k2 = 0x0101; // 2nd byte is the same as above

        // Two distinct ColoringData values with non-zero cumulative fields
        let cd1 = ColoringData {
            blue_score: 10,
            blue_work: Default::default(),
            selected_parent: Hash::from_u64_word(1),
            mergeset_blues: Default::default(),
            mergeset_reds: Default::default(),
            blues_anticone_sizes: Default::default(),
        };
        let cd2 = ColoringData {
            blue_score: 20,
            blue_work: Default::default(),
            selected_parent: Hash::from_u64_word(2),
            mergeset_blues: Default::default(),
            mergeset_reds: Default::default(),
            blues_anticone_sizes: Default::default(),
        };

        let key1 = ConflictZoneColoringKey::new(root, pov, k1, false);
        let key2 = ConflictZoneColoringKey::new(root, pov, k2, false);

        // Insert both into the DB-backed store
        store.insert(key1.clone(), Arc::new(cd1)).expect("insert k1");
        store.insert(key2.clone(), Arc::new(cd2)).expect("insert k2");

        // Read them back and verify isolation
        let read1 = store.get_data(key1).expect("read k1");
        let read2 = store.get_data(key2).expect("read k2");

        assert_eq!(read1.blue_score, 10);
        assert_eq!(read2.blue_score, 20);
    }
}
