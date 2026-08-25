use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Per-block SMT metadata. Wire: `[Hash || u64 || Hash]` = 72 bytes.
///
/// `inactivity_shortcut_block` is retained per-row because
/// `compute_inactivity_shortcut_block`'s parent-fallback path consults the
/// parent's stored row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmtBlockMetadata {
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
    pub inactivity_shortcut_block: Hash,
}

impl SmtBlockMetadata {
    pub fn new(payload_and_ctx_digest: Hash, inactivity_shortcut_block: Hash, active_lanes_count: u64) -> Self {
        Self { payload_and_ctx_digest, active_lanes_count, inactivity_shortcut_block }
    }

    pub fn payload_and_ctx_digest(&self) -> Hash {
        self.payload_and_ctx_digest
    }

    pub fn inactivity_shortcut_block(&self) -> Hash {
        self.inactivity_shortcut_block
    }

    pub fn active_lanes_count(&self) -> u64 {
        self.active_lanes_count
    }
}

impl MemSizeEstimator for SmtBlockMetadata {}

/// Block-hash-keyed metadata store with in-memory cache. Key = `[prefix || hash]`.
#[derive(Clone)]
pub struct DbSmtMetadataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, SmtBlockMetadata, BlockHasher>,
}

impl DbSmtMetadataStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::SmtSeqCommitMeta.into()), db }
    }

    pub fn get(&self, block_hash: Hash) -> StoreResult<SmtBlockMetadata> {
        self.access.read(block_hash)
    }

    pub fn has(&self, block_hash: Hash) -> StoreResult<bool> {
        self.access.has(block_hash)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, block_hash: Hash, metadata: SmtBlockMetadata) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), block_hash, metadata)
    }

    pub fn delete_all(&self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, block_hash: Hash) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), block_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn row_key(hash: Hash) -> Vec<u8> {
        let mut key = vec![DatabaseStorePrefixes::SmtSeqCommitMeta.into()];
        key.extend_from_slice(hash.as_bytes().as_ref());
        key
    }

    #[test]
    fn round_trip() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbSmtMetadataStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(0xDEAD_BEEF);
        let meta = SmtBlockMetadata::new(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32]), 42);

        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, hash, meta).unwrap();
        db.write(batch).unwrap();

        let on_disk = db.get_pinned(row_key(hash)).unwrap().unwrap();
        assert_eq!(on_disk.len(), 72);
        assert_eq!(store.get(hash).unwrap(), meta);
    }

    #[test]
    fn decode_handcrafted_wire() {
        let mut bytes = Vec::with_capacity(72);
        bytes.extend_from_slice(&[0x11; 32]);
        bytes.extend_from_slice(&42u64.to_le_bytes());
        bytes.extend_from_slice(&[0x22; 32]);
        let decoded: SmtBlockMetadata = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            decoded,
            SmtBlockMetadata {
                payload_and_ctx_digest: Hash::from_bytes([0x11; 32]),
                active_lanes_count: 42,
                inactivity_shortcut_block: Hash::from_bytes([0x22; 32]),
            }
        );
    }

    #[test]
    fn accessors_work() {
        let metadata = SmtBlockMetadata::new(Hash::from_bytes([0xAA; 32]), Hash::from_bytes([0x22; 32]), 7);
        assert_eq!(metadata.payload_and_ctx_digest(), Hash::from_bytes([0xAA; 32]));
        assert_eq!(metadata.inactivity_shortcut_block(), Hash::from_bytes([0x22; 32]));
        assert_eq!(metadata.active_lanes_count(), 7);
    }

    #[test]
    fn wire_size() {
        let metadata = SmtBlockMetadata::new(Hash::from_bytes([0; 32]), Hash::from_bytes([0; 32]), 0);
        assert_eq!(bincode::serialize(&metadata).unwrap().len(), 72);
    }

    #[test]
    fn delete_clears_row() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbSmtMetadataStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(1);
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, hash, SmtBlockMetadata::new(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32]), 99)).unwrap();
        db.write(batch).unwrap();

        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, hash).unwrap();
        db.write(batch).unwrap();

        assert!(db.get_pinned(row_key(hash)).unwrap().is_none());
    }
}
