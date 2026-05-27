use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::de::{Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

/// Pre-anchor (Toccata) per-block SMT metadata. Wire: `[Hash || u64]` = 40 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToccataV0 {
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
}

/// Post-anchor per-block SMT metadata. Wire: `[Hash || u64 || Hash]` = 72 bytes.
///
/// Same first 40 bytes as `ToccataV0`; the trailing `inactivity_shortcut_block`
/// is the only structural difference. `inactivity_shortcut_block` is retained
/// per-row because `compute_inactivity_shortcut_block`'s parent-fallback path
/// consults the parent's stored row; it does NOT feed `mergeset_context_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToccataV1 {
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
    pub inactivity_shortcut_block: Hash,
}

/// Per-block SMT metadata, length-discriminated on the wire (40 vs 72 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmtBlockMetadata {
    ToccataV0(ToccataV0),
    ToccataV1(ToccataV1),
}

impl SmtBlockMetadata {
    /// Construct a `ToccataV1` variant. The only variant new code ever writes.
    pub fn new(payload_and_ctx_digest: Hash, inactivity_shortcut_block: Hash, active_lanes_count: u64) -> Self {
        Self::ToccataV1(ToccataV1 { payload_and_ctx_digest, active_lanes_count, inactivity_shortcut_block })
    }

    pub fn payload_and_ctx_digest(&self) -> Hash {
        match self {
            Self::ToccataV0(v) => v.payload_and_ctx_digest,
            Self::ToccataV1(v) => v.payload_and_ctx_digest,
        }
    }

    pub fn inactivity_shortcut_block(&self) -> Option<Hash> {
        match self {
            Self::ToccataV1(v) => Some(v.inactivity_shortcut_block),
            Self::ToccataV0(_) => None,
        }
    }

    pub fn active_lanes_count(&self) -> u64 {
        match self {
            Self::ToccataV1(v) => v.active_lanes_count,
            Self::ToccataV0(v) => v.active_lanes_count,
        }
    }
}

impl MemSizeEstimator for SmtBlockMetadata {}

impl Serialize for SmtBlockMetadata {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ToccataV0(v) => v.serialize(s),
            Self::ToccataV1(v) => v.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for SmtBlockMetadata {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = SmtBlockMetadata;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("SmtBlockMetadata: [Hash, u64] (ToccataV0) or [Hash, u64, Hash] (ToccataV1)")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let first: Hash = seq.next_element()?.ok_or_else(|| A::Error::custom("missing first hash"))?;
                let count: u64 = seq.next_element()?.ok_or_else(|| A::Error::custom("missing active_lanes_count"))?;
                // Third element discriminates: a Hash present => V1; EOF on the
                // third read => V0 (no trailing inactivity_shortcut_block). On a
                // 40-byte V0 wire the buffer is empty before this read, so no
                // bytes are partially consumed; the error path is safe.
                match seq.next_element::<Hash>() {
                    Ok(Some(inactivity_shortcut_block)) => Ok(SmtBlockMetadata::ToccataV1(ToccataV1 {
                        payload_and_ctx_digest: first,
                        active_lanes_count: count,
                        inactivity_shortcut_block,
                    })),
                    Ok(None) | Err(_) => {
                        Ok(SmtBlockMetadata::ToccataV0(ToccataV0 { payload_and_ctx_digest: first, active_lanes_count: count }))
                    }
                }
            }
        }
        d.deserialize_tuple(3, V)
    }
}

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
    fn round_trip_v1() {
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
    fn round_trip_v0() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbSmtMetadataStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(0xABCD);
        let meta =
            SmtBlockMetadata::ToccataV0(ToccataV0 { payload_and_ctx_digest: Hash::from_bytes([0xAA; 32]), active_lanes_count: 7 });

        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, hash, meta).unwrap();
        db.write(batch).unwrap();

        let on_disk = db.get_pinned(row_key(hash)).unwrap().unwrap();
        assert_eq!(on_disk.len(), 40);
        assert_eq!(store.get(hash).unwrap(), meta);
    }

    #[test]
    fn decode_handcrafted_v0_wire() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbSmtMetadataStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(0xABCD);
        let mut bytes = Vec::with_capacity(40);
        bytes.extend_from_slice(&[0xAA; 32]);
        bytes.extend_from_slice(&7u64.to_le_bytes());
        db.put(row_key(hash), bytes).unwrap();

        let decoded = store.get(hash).unwrap();
        assert_eq!(
            decoded,
            SmtBlockMetadata::ToccataV0(ToccataV0 { payload_and_ctx_digest: Hash::from_bytes([0xAA; 32]), active_lanes_count: 7 })
        );
        assert_eq!(decoded.active_lanes_count(), 7);
    }

    #[test]
    fn decode_handcrafted_v1_wire() {
        let mut bytes = Vec::with_capacity(72);
        bytes.extend_from_slice(&[0x11; 32]);
        bytes.extend_from_slice(&42u64.to_le_bytes());
        bytes.extend_from_slice(&[0x22; 32]);
        let decoded: SmtBlockMetadata = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            decoded,
            SmtBlockMetadata::ToccataV1(ToccataV1 {
                payload_and_ctx_digest: Hash::from_bytes([0x11; 32]),
                active_lanes_count: 42,
                inactivity_shortcut_block: Hash::from_bytes([0x22; 32]),
            })
        );
    }

    #[test]
    fn payload_and_ctx_digest_accessor_works_for_both_variants() {
        let v0 =
            SmtBlockMetadata::ToccataV0(ToccataV0 { payload_and_ctx_digest: Hash::from_bytes([0xAA; 32]), active_lanes_count: 0 });
        let v1 = SmtBlockMetadata::new(Hash::from_bytes([0xAA; 32]), Hash::from_bytes([0x22; 32]), 0);
        assert_eq!(v0.payload_and_ctx_digest(), Hash::from_bytes([0xAA; 32]));
        assert_eq!(v1.payload_and_ctx_digest(), Hash::from_bytes([0xAA; 32]));
    }

    #[test]
    fn variant_wire_sizes() {
        let v0 = SmtBlockMetadata::ToccataV0(ToccataV0 { payload_and_ctx_digest: Hash::from_bytes([0; 32]), active_lanes_count: 0 });
        let v1 = SmtBlockMetadata::new(Hash::from_bytes([0; 32]), Hash::from_bytes([0; 32]), 0);
        assert_eq!(bincode::serialize(&v0).unwrap().len(), 40);
        assert_eq!(bincode::serialize(&v1).unwrap().len(), 72);
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
