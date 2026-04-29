use std::sync::Arc;

use kaspa_consensus_core::{BlockHasher, utxo::pre_toccata::PreToccataUtxoDiff, utxo::utxo_diff::UtxoDiff};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Version suffix appended to every post-Toccata `UtxoDiffs` row's DB key.
/// Pre-Toccata rows have no suffix and are decoded through [`PreToccataUtxoDiff`]
/// by the version-aware path in `CachedDbAccess`.
pub const POST_TOCCATA_UTXO_DIFFS_VERSION: u8 = 1;

/// Store for holding the UTXO difference (delta) of a block relative to its selected parent.
/// Note that this data is lazy-computed only for blocks which are candidates to being chain
/// blocks. However, once the diff is computed, it is permanent. This store has a relation to
/// block status, such that if a block has status `StatusUTXOValid` then it is expected to have
/// utxo diff data as well as utxo multiset data and acceptance data.
pub trait UtxoDiffsStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<UtxoDiff>, StoreError>;
}

pub trait UtxoDiffsStore: UtxoDiffsStoreReader {
    fn insert(&self, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `UtxoDifferencesStore` trait, with concurrency support.
///
/// Writes go under the post-Toccata versioned key layout `[prefix || hash || 1]`.
/// Reads transparently handle both layouts: pre-Toccata rows (no version suffix) are
/// decoded via [`PreToccataUtxoDiff`] and converted to `Arc<UtxoDiff>` with every
/// entry's `covenant_id` defaulting to `None`. See the module-level docs on
/// `kaspa_database::prelude::CachedDbAccess` for the full semantics.
#[derive(Clone)]
pub struct DbUtxoDiffsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<UtxoDiff>, BlockHasher, PreToccataUtxoDiff>,
}

impl DbUtxoDiffsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new_with_version_suffix(
                db,
                cache_policy,
                DatabaseStorePrefixes::UtxoDiffs.into(),
                POST_TOCCATA_UTXO_DIFFS_VERSION,
            ),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, utxo_diff)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl UtxoDiffsStoreReader for DbUtxoDiffsStore {
    fn get(&self, hash: Hash) -> Result<Arc<UtxoDiff>, StoreError> {
        self.access.read(hash)
    }
}

impl UtxoDiffsStore for DbUtxoDiffsStore {
    fn insert(&self, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, utxo_diff)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

#[cfg(test)]
mod tests {
    //! End-to-end compat tests for `DbUtxoDiffsStore` across the Toccata
    //! version boundary. These drive a real RocksDB and exercise every
    //! layer: the store's trait surface, the versioned access layer, the
    //! `PreToccataUtxoDiff` shadow decoder, and the row-level key layout.
    use super::*;
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
    use kaspa_consensus_core::utxo::utxo_diff::UtxoDiff;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_database::prelude::StoreErrorPredicates;
    use std::collections::HashMap;

    /// Hex bytes captured on the `master` worktree from a 2-add / 2-remove
    /// pre-Toccata `UtxoDiff`. Same fixture as in
    /// `consensus/core/src/utxo/utxo_diff.rs`; duplicating it here keeps the
    /// integration test self-contained and independent of any internals of
    /// the core crate's test module.
    const PRE_TOCCATA_HEX: &str = "0200000000000000111111111111111111111111111111111111111111111111111111111111111100000000efcdab89674523010000060000000000000076a9140102032a0000000000000001333333333333333333333333333333333333333333333333333333333333333304000000f40100000000000000000200000000000000aabb640000000000000000020000000000000044444444444444444444444444444444444444444444444444444444444444440b000000090300000000000000000400000000000000deadbeef0c000000000000000122222222222222222222222222222222222222222222222222222222222222220700000080841e0000000000000002000000000000005152630000000000000000";

    fn pre_toccata_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; PRE_TOCCATA_HEX.len() / 2];
        faster_hex::hex_decode(PRE_TOCCATA_HEX.as_bytes(), &mut bytes).unwrap();
        bytes
    }

    /// Legacy (unversioned) DB key for a given block hash: `[prefix || hash]`.
    fn legacy_row_key(hash: Hash) -> Vec<u8> {
        let mut key = vec![DatabaseStorePrefixes::UtxoDiffs.into()];
        key.extend_from_slice(hash.as_bytes().as_ref());
        key
    }

    /// Versioned DB key for a given block hash: `[prefix || hash || version_suffix]`.
    fn versioned_row_key(hash: Hash) -> Vec<u8> {
        let mut key = legacy_row_key(hash);
        key.push(POST_TOCCATA_UTXO_DIFFS_VERSION);
        key
    }

    fn assert_pre_toccata_diff(diff: &UtxoDiff) {
        assert_eq!(diff.add.len(), 2);
        assert_eq!(diff.remove.len(), 2);

        let op_add_a = TransactionOutpoint::new(Hash::from_bytes([0x11; 32]), 0);
        let op_add_b = TransactionOutpoint::new(Hash::from_bytes([0x33; 32]), 4);
        let op_remove_a = TransactionOutpoint::new(Hash::from_bytes([0x22; 32]), 7);
        let op_remove_b = TransactionOutpoint::new(Hash::from_bytes([0x44; 32]), 11);

        let add_a = diff.add.get(&op_add_a).expect("add entry a present");
        assert_eq!(add_a.amount, 0x0123_4567_89ab_cdef);
        assert_eq!(add_a.block_daa_score, 42);
        assert!(add_a.is_coinbase);
        assert_eq!(add_a.script_public_key.script(), [0x76, 0xa9, 0x14, 0x01, 0x02, 0x03].as_slice());
        assert_eq!(add_a.covenant_id, None, "pre-Toccata rows must come back with covenant_id == None");

        let add_b = diff.add.get(&op_add_b).expect("add entry b present");
        assert_eq!(add_b.amount, 500);
        assert_eq!(add_b.block_daa_score, 100);
        assert!(!add_b.is_coinbase);
        assert_eq!(add_b.script_public_key.script(), [0xaa, 0xbb].as_slice());
        assert_eq!(add_b.covenant_id, None);

        let remove_a = diff.remove.get(&op_remove_a).expect("remove entry a present");
        assert_eq!(remove_a.amount, 2_000_000);
        assert_eq!(remove_a.block_daa_score, 99);
        assert!(!remove_a.is_coinbase);
        assert_eq!(remove_a.script_public_key.script(), [0x51, 0x52].as_slice());
        assert_eq!(remove_a.covenant_id, None);

        let remove_b = diff.remove.get(&op_remove_b).expect("remove entry b present");
        assert_eq!(remove_b.amount, 777);
        assert_eq!(remove_b.block_daa_score, 12);
        assert!(remove_b.is_coinbase);
        assert_eq!(remove_b.script_public_key.script(), [0xde, 0xad, 0xbe, 0xef].as_slice());
        assert_eq!(remove_b.covenant_id, None);
    }

    /// Drives a pre-Toccata UtxoDiff blob through the live store: plant
    /// raw bytes under the unversioned `[prefix || hash]` layout and read
    /// through `DbUtxoDiffsStore::get`. The store must decode via
    /// `PreToccataUtxoDiff` and expose the row as an `Arc<UtxoDiff>` with
    /// every entry's `covenant_id == None`.
    #[test]
    fn get_decodes_pre_toccata_row() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoDiffsStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(0xABCD_1234_5678_9ABC);
        db.put(legacy_row_key(hash), pre_toccata_bytes()).unwrap();

        let diff = store.get(hash).expect("decode pre-Toccata row through the store");
        assert_pre_toccata_diff(&diff);

        // Reading twice must be idempotent (the cache slot should now hold the
        // decoded post-Toccata `Arc<UtxoDiff>`; the next read skips the scan).
        let diff2 = store.get(hash).unwrap();
        assert!(Arc::ptr_eq(&diff, &diff2), "cache must return the same Arc after the first read");
    }

    /// A post-Toccata write through the store lands under the versioned
    /// `[prefix || hash || version_suffix]` layout and round-trips via
    /// `get`.
    #[test]
    fn post_toccata_round_trip_uses_versioned_layout() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoDiffsStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(0xDEAD_BEEF);

        let mut add = HashMap::new();
        add.insert(
            TransactionOutpoint::new(Hash::from_bytes([0x77; 32]), 3),
            UtxoEntry::new(
                1_234_567,
                ScriptPublicKey::new(0, [0xa1, 0xa2, 0xa3].as_slice().iter().copied().collect()),
                88,
                false,
                Some(Hash::from_bytes([0x5a; 32])),
            ),
        );
        let diff = Arc::new(UtxoDiff { add, remove: Default::default() });

        store.insert(hash, Arc::clone(&diff)).unwrap();

        // The row lives under the versioned key, NOT the legacy one.
        assert!(db.get_pinned(versioned_row_key(hash)).unwrap().is_some());
        assert!(db.get_pinned(legacy_row_key(hash)).unwrap().is_none());

        // Round trip.
        let round = store.get(hash).unwrap();
        assert_eq!(*round, *diff);
    }

    /// `insert` must refuse to overwrite a pre-Toccata row — otherwise a
    /// legacy row and a versioned row could coexist for the same logical
    /// key and break `read_versioned`'s invariant (1).
    #[test]
    fn insert_rejects_existing_legacy_row() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoDiffsStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash = Hash::from_u64_word(123);
        db.put(legacy_row_key(hash), pre_toccata_bytes()).unwrap();

        let empty_diff = Arc::new(UtxoDiff::default());
        let err = store.insert(hash, empty_diff).unwrap_err();
        assert!(err.is_already_exists(), "expected HashAlreadyExists, got {err:?}");
    }

    /// `delete` must clear both layouts so legacy rows cannot be
    /// resurrected after the key has been deleted.
    #[test]
    fn delete_clears_both_legacy_and_versioned_rows() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoDiffsStore::new(Arc::clone(&db), CachePolicy::Count(16));

        let hash_legacy = Hash::from_u64_word(1);
        let hash_versioned = Hash::from_u64_word(2);

        // Pre-Toccata row planted directly.
        db.put(legacy_row_key(hash_legacy), pre_toccata_bytes()).unwrap();
        // Post-Toccata row written through the store.
        store.insert(hash_versioned, Arc::new(UtxoDiff::default())).unwrap();

        store.delete(hash_legacy).unwrap();
        store.delete(hash_versioned).unwrap();

        assert!(db.get_pinned(legacy_row_key(hash_legacy)).unwrap().is_none());
        assert!(db.get_pinned(versioned_row_key(hash_versioned)).unwrap().is_none());
        assert!(store.get(hash_legacy).unwrap_err().is_key_not_found());
        assert!(store.get(hash_versioned).unwrap_err().is_key_not_found());
    }
}
