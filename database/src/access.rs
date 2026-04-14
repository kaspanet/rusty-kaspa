use crate::{cache::CachePolicy, db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::{Direction, IterateBounds, IteratorMode, ReadOptions};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::hash_map::RandomState, error::Error, hash::BuildHasher, marker::PhantomData, sync::Arc};

/// A concurrent DB store access with typed caching.
///
/// # Versioned stores
///
/// `CachedDbAccess` supports an opt-in "versioned key" mode used by stores
/// whose on-disk value layout changed across a hardfork (currently only
/// `DbUtxoDiffsStore` across Toccata). A version-aware store is constructed
/// with [`CachedDbAccess::new_with_version_suffix`].
///
/// When a version suffix is configured, every write appends that byte to the
/// DB key and every read uses a `rocksdb::PrefixRange` scan on
/// `[store_prefix || logical_key]` so pre-fork (unversioned) and post-fork
/// (versioned) rows can coexist on disk under the same logical key. Legacy
/// rows decode through a caller-provided shadow type `TLegacy` and are
/// converted into the live `TData` via `TLegacy: Into<TData>`. When
/// `version_suffix` is `None` (the default for every existing store) each
/// method falls through to the pre-fork point-get path unchanged — zero
/// functional impact on non-versioned stores.
///
/// The `TLegacy` generic defaults to `TData`, so existing callers that spell
/// only two or three type parameters (`TKey, TData[, S]`) continue to
/// compile unchanged: `TData: Into<TData>` is supplied by the blanket
/// `impl<T> From<T> for T`.
#[derive(Clone)]
pub struct CachedDbAccess<TKey, TData, S = RandomState, TLegacy = TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, TData, S>,

    // DB bucket/path
    prefix: Vec<u8>,

    /// When `Some(v)`, this store is version-aware: writes append `v` to the
    /// DB key and reads use a prefix scan that also matches legacy
    /// (unversioned) rows. See the struct-level docs for full semantics.
    version_suffix: Option<u8>,

    /// Marker for the legacy-decoder type. Only consulted inside
    /// `read_versioned` when a legacy row is found on disk.
    _legacy: PhantomData<fn() -> TLegacy>,
}

pub type KeyDataResult<TData> = Result<(Box<[u8]>, TData), Box<dyn Error>>;

impl<TKey, TData, S, TLegacy> CachedDbAccess<TKey, TData, S, TLegacy>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>) -> Self {
        Self { db, cache: Cache::new(cache_policy), prefix, version_suffix: None, _legacy: PhantomData }
    }

    /// Constructs a version-aware store. Writes append `version_suffix` to
    /// every DB key. Reads use a `rocksdb::PrefixRange` scan on
    /// `[store_prefix || logical_key]` and dispatch on the tail byte:
    ///
    /// - no tail byte → legacy (pre-fork) row, decoded as `TLegacy` and
    ///   converted into `TData` via `TLegacy: Into<TData>`;
    /// - tail byte == `version_suffix` → current row, decoded directly as
    ///   `TData`;
    /// - any other tail → [`StoreError::DataInconsistency`].
    ///
    /// The caller is responsible for picking a `TLegacy` type whose derived
    /// `Deserialize` matches the pre-fork byte layout, and for providing a
    /// `From<TLegacy> for TData` conversion. See
    /// `consensus/core/src/utxo/pre_toccata.rs` for the current example.
    pub fn new_with_version_suffix(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>, version_suffix: u8) -> Self {
        Self { db, cache: Cache::new(cache_policy), prefix, version_suffix: Some(version_suffix), _legacy: PhantomData }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<TData>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        self.cache.get(&key)
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        if self.cache.contains_key(&key) {
            return Ok(true);
        }
        if self.version_suffix.is_some() {
            // Version-aware existence check: any row (legacy or current) with
            // the scan prefix counts as "has". We do not need to decode the
            // value or validate the suffix byte — presence alone is enough.
            // This is the guard that upholds `read_versioned`'s invariant (1):
            // `insert`/`insert_batch` call `has` first and refuse to overwrite,
            // so legacy rows cannot be shadowed by new versioned writes.
            let scan_prefix = DbKey::new(&self.prefix, key);
            let mut read_opts = ReadOptions::default();
            read_opts.set_iterate_range(rocksdb::PrefixRange(scan_prefix.as_ref()));
            let mut iter = self.db.iterator_opt(IteratorMode::From(scan_prefix.as_ref(), Direction::Forward), read_opts);
            return match iter.next() {
                Some(Ok(_)) => Ok(true),
                Some(Err(e)) => Err(e.into()),
                None => Ok(false),
            };
        }
        Ok(self.db.get_pinned(DbKey::new(&self.prefix, key))?.is_some())
    }

    pub fn read(&self, key: TKey) -> Result<TData, StoreError>
    where
        TKey: Clone + AsRef<[u8]> + ToString,
        TData: DeserializeOwned,
        TLegacy: DeserializeOwned + Into<TData>,
    {
        if let Some(data) = self.cache.get(&key) {
            return Ok(data);
        }
        if let Some(suffix) = self.version_suffix {
            return self.read_versioned(key, suffix);
        }
        let db_key = DbKey::new(&self.prefix, key.clone());
        if let Some(slice) = self.db.get_pinned(&db_key)? {
            let data: TData = bincode::deserialize(&slice)?;
            self.cache.insert(key, data.clone());
            Ok(data)
        } else {
            Err(StoreError::KeyNotFound(db_key))
        }
    }

    /// Reads a row from a version-aware store.
    ///
    /// # Invariants this method assumes
    ///
    /// 1. **At most one physical row per logical key.** A logical key is
    ///    either stored under the legacy (pre-fork) layout
    ///    `[store_prefix || logical_key]` or under the current layout
    ///    `[store_prefix || logical_key || version_suffix]`. It is never
    ///    stored under both simultaneously and never stored under two
    ///    different versions. The `insert`/`insert_batch` paths on the
    ///    owning store uphold this invariant by calling `has()`
    ///    (version-aware) before every write and refusing to overwrite;
    ///    `delete`/`delete_many` always clear both layouts so no orphan
    ///    can persist.
    ///
    /// 2. **RocksDB `PrefixRange` iteration is ordered and bounded.** The
    ///    iterator yields keys in lexicographic order, restricted to those
    ///    starting with `scan_bytes`. We trust the bound and do not
    ///    re-check `starts_with` on returned keys.
    ///
    /// 3. **No key length other than `scan_len` or `scan_len + 1` can
    ///    legitimately exist.** Anything else is corruption or a future
    ///    version this binary does not understand — in both cases the
    ///    right answer is to surface an error rather than silently fall
    ///    through.
    ///
    /// # Behaviour
    ///
    /// A single `PrefixRange([store_prefix || logical_key])` scan is opened
    /// and we take exactly its first (and, by invariant 1, only) element:
    ///
    /// - **No element** → [`StoreError::KeyNotFound`].
    /// - **Element key length == `scan_len`** → legacy row. Decode the value
    ///   bytes as `TLegacy` and convert via `TLegacy: Into<TData>`. The row
    ///   is **not** rewritten under the versioned layout — it stays on disk
    ///   until the key is deleted or explicitly re-inserted.
    /// - **Element key length == `scan_len + 1`** → versioned row. The tail
    ///   byte must equal the configured `suffix`; any mismatch is a
    ///   data-inconsistency error (this binary does not recognize the
    ///   stored version). On match, decode directly as `TData`.
    /// - **Any other length** → data-inconsistency error.
    ///
    /// The decoded value is inserted into the cache under the caller's
    /// logical `TKey`, so subsequent reads skip the prefix scan entirely.
    /// The cache is version-agnostic — it holds the live `TData` regardless
    /// of which layout produced it.
    fn read_versioned(&self, key: TKey, suffix: u8) -> Result<TData, StoreError>
    where
        TKey: Clone + AsRef<[u8]> + ToString,
        TData: DeserializeOwned,
        TLegacy: DeserializeOwned + Into<TData>,
    {
        // Build the scan prefix = [store_prefix || logical_key]. Both the
        // legacy and versioned physical rows share this byte sequence.
        let scan_prefix = DbKey::new(&self.prefix, key.clone());
        let scan_bytes = scan_prefix.as_ref();
        let scan_len = scan_bytes.len();

        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(scan_bytes));
        let mut iter = self.db.iterator_opt(IteratorMode::From(scan_bytes, Direction::Forward), read_opts);

        // By invariant (1) there is at most one matching row. Pull exactly
        // one element and decide what to do based solely on its key length.
        let Some(first) = iter.next() else {
            return Err(StoreError::KeyNotFound(scan_prefix));
        };
        let (found_key, found_bytes) = first?;

        let data: TData = if found_key.len() == scan_len {
            // Legacy (pre-fork) row: empty tail → decode via the legacy
            // codec and convert into the live `TData`.
            let legacy_value: TLegacy = bincode::deserialize(&found_bytes)?;
            legacy_value.into()
        } else if found_key.len() == scan_len + 1 {
            // Versioned row: single tail byte that must match the store's
            // configured version. A mismatch is a data-inconsistency error
            // — this binary does not recognize the stored version and
            // refusing to decode is safer than guessing.
            let tail = found_key[scan_len];
            if tail != suffix {
                return Err(StoreError::DataInconsistency(format!(
                    "unexpected version byte {tail:#x} for key {scan_prefix}, expected {suffix:#x}"
                )));
            }
            bincode::deserialize(&found_bytes)?
        } else {
            // Neither layout matches — corruption or an unknown future variant.
            return Err(StoreError::DataInconsistency(format!(
                "unexpected row key length {} for {scan_prefix}, expected {} (legacy) or {} (versioned)",
                found_key.len(),
                scan_len,
                scan_len + 1,
            )));
        };

        self.cache.insert(key, data.clone());
        Ok(data)
    }

    /// Note: `has_with_fallback` and [`Self::read_with_fallback`] are the
    /// prefix-migration helpers, orthogonal to the version-suffix mechanism.
    /// They are not aware of `version_suffix` and are not intended to be
    /// combined with a versioned store; use [`Self::has`] / [`Self::read`]
    /// for versioned reads.
    pub fn has_with_fallback(&self, fallback_prefix: &[u8], key: TKey) -> Result<bool, StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        if self.cache.contains_key(&key) {
            Ok(true)
        } else {
            let db_key = DbKey::new(&self.prefix, key.clone());
            if self.db.get_pinned(&db_key)?.is_some() {
                Ok(true)
            } else {
                let db_key = DbKey::new(fallback_prefix, key.clone());
                Ok(self.db.get_pinned(&db_key)?.is_some())
            }
        }
    }

    /// See [`Self::has_with_fallback`] for the note on interaction with
    /// versioned stores.
    pub fn read_with_fallback<TFallbackDeser>(&self, fallback_prefix: &[u8], key: TKey) -> Result<TData, StoreError>
    where
        TKey: Clone + AsRef<[u8]> + ToString,
        TData: DeserializeOwned,
        TFallbackDeser: DeserializeOwned + Into<TData>,
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let db_key = DbKey::new(&self.prefix, key.clone());
            if let Some(slice) = self.db.get_pinned(&db_key)? {
                let data: TData = bincode::deserialize(&slice)?;
                self.cache.insert(key, data.clone());
                Ok(data)
            } else {
                let db_key = DbKey::new(fallback_prefix, key.clone());
                if let Some(slice) = self.db.get_pinned(&db_key)? {
                    let data: TFallbackDeser = bincode::deserialize(&slice)?;
                    let data: TData = data.into();
                    self.cache.insert(key, data.clone());
                    Ok(data)
                } else {
                    Err(StoreError::KeyNotFound(db_key))
                }
            }
        }
    }

    /// Iterates every row in this store under the bare `[store_prefix || *]`
    /// range. **Not yet aware of versioned stores:** on a version-aware
    /// store this will emit logical keys that still carry the trailing
    /// version suffix byte, and it does not dispatch legacy rows through
    /// `TLegacy`. `DbUtxoDiffsStore` does not call this method; if a future
    /// versioned store needs streaming iteration, this helper will have to
    /// learn about `version_suffix` first.
    pub fn iterator(&self) -> impl Iterator<Item = KeyDataResult<TData>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        let prefix_key = DbKey::prefix_only(&self.prefix);
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(prefix_key.as_ref()));
        self.db.iterator_opt(IteratorMode::From(prefix_key.as_ref(), Direction::Forward), read_opts).map(move |iter_result| {
            match iter_result {
                Ok((key, data_bytes)) => match bincode::deserialize(&data_bytes) {
                    Ok(data) => Ok((key[prefix_key.prefix_len()..].into(), data)),
                    Err(e) => Err(e.into()),
                },
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn write(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        let bin_data = bincode::serialize(&data)?;
        self.cache.insert(key.clone(), data);
        writer.put(self.build_write_key(key), bin_data)?;
        Ok(())
    }

    pub fn write_many(
        &self,
        mut writer: impl DbWriter,
        iter: &mut (impl Iterator<Item = (TKey, TData)> + Clone),
    ) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        let iter_clone = iter.clone();
        self.cache.insert_many(iter);
        for (key, data) in iter_clone {
            let bin_data = bincode::serialize(&data)?;
            writer.put(self.build_write_key(key), bin_data)?;
        }
        Ok(())
    }

    /// Write directly from an iterator and do not cache any data. NOTE: this action also clears the cache
    pub fn write_many_without_cache(
        &self,
        mut writer: impl DbWriter,
        iter: &mut impl Iterator<Item = (TKey, TData)>,
    ) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        for (key, data) in iter {
            let bin_data = bincode::serialize(&data)?;
            writer.put(self.build_write_key(key), bin_data)?;
        }
        // We must clear the cache in order to avoid invalidated entries
        self.cache.remove_all();
        Ok(())
    }

    pub fn delete(&self, mut writer: impl DbWriter, key: TKey) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.cache.remove(&key);
        self.delete_both_layouts(&mut writer, key)
    }

    pub fn delete_many(&self, mut writer: impl DbWriter, key_iter: &mut (impl Iterator<Item = TKey> + Clone)) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        let key_iter_clone = key_iter.clone();
        self.cache.remove_many(key_iter);
        for key in key_iter_clone {
            self.delete_both_layouts(&mut writer, key)?;
        }
        Ok(())
    }

    /// Builds the DB key used for writes. For non-versioned stores this is
    /// `[store_prefix || logical_key]`; for versioned stores it is
    /// `[store_prefix || logical_key || version_suffix]`.
    fn build_write_key(&self, key: TKey) -> DbKey
    where
        TKey: Clone + AsRef<[u8]>,
    {
        let mut db_key = DbKey::new(&self.prefix, key);
        if let Some(suffix) = self.version_suffix {
            db_key.add_suffix([suffix]);
        }
        db_key
    }

    /// Deletes both the versioned and legacy rows for a logical key. Delete
    /// of a non-existent row is a no-op in RocksDB, so non-versioned stores
    /// still issue exactly one delete and versioned stores issue one extra
    /// no-op delete during the migration window.
    fn delete_both_layouts(&self, writer: &mut impl DbWriter, key: TKey) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        if let Some(suffix) = self.version_suffix {
            let mut versioned_key = DbKey::new(&self.prefix, key.clone());
            versioned_key.add_suffix([suffix]);
            writer.delete(versioned_key)?;
        }
        writer.delete(DbKey::new(&self.prefix, key))?;
        Ok(())
    }

    /// Deletes all entries in the store using the underlying rocksdb `delete_range` operation
    pub fn delete_all(&self, mut writer: impl DbWriter) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.cache.remove_all();
        let db_key = DbKey::prefix_only(&self.prefix);
        let (from, to) = rocksdb::PrefixRange(db_key.as_ref()).into_bounds();
        writer.delete_range(from.unwrap(), to.unwrap())?;
        Ok(())
    }

    /// A dynamic iterator that can iterate through a specific prefix / bucket, or from a certain start point.
    ///
    /// Not yet aware of versioned stores — same caveat as [`Self::iterator`].
    //TODO: loop and chain iterators for multi-prefix / bucket iterator.
    pub fn seek_iterator(
        &self,
        bucket: Option<&[u8]>,   // iter self.prefix if None, else append bytes to self.prefix.
        seek_from: Option<TKey>, // iter whole range if None
        limit: usize,            // amount to take.
        skip_first: bool,        // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = KeyDataResult<TData>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        let db_key = bucket.map_or_else(
            move || DbKey::prefix_only(&self.prefix),
            move |bucket| {
                let mut key = DbKey::prefix_only(&self.prefix);
                key.add_bucket(bucket);
                key
            },
        );

        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(db_key.as_ref()));

        let mut db_iterator = match seek_from {
            Some(seek_key) => {
                self.db.iterator_opt(IteratorMode::From(DbKey::new(&self.prefix, seek_key).as_ref(), Direction::Forward), read_opts)
            }
            None => self.db.iterator_opt(IteratorMode::Start, read_opts),
        };

        if skip_first {
            db_iterator.next();
        }

        db_iterator.take(limit).map(move |item| match item {
            Ok((key_bytes, value_bytes)) => match bincode::deserialize::<TData>(value_bytes.as_ref()) {
                Ok(value) => Ok((key_bytes[db_key.prefix_len()..].into(), value)),
                Err(err) => Err(err.into()),
            },
            Err(err) => Err(err.into()),
        })
    }

    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, DirectDbWriter},
    };
    use kaspa_hashes::Hash;
    use rocksdb::WriteBatch;

    #[test]
    fn test_delete_all() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(2), vec![1, 2]);

        access.write_many(DirectDbWriter::new(&db), &mut (0..16).map(|i| (i.into(), 2))).unwrap();
        assert_eq!(16, access.iterator().count());
        access.delete_all(DirectDbWriter::new(&db)).unwrap();
        assert_eq!(0, access.iterator().count());

        access.write_many(DirectDbWriter::new(&db), &mut (0..16).map(|i| (i.into(), 2))).unwrap();
        assert_eq!(16, access.iterator().count());
        let mut batch = WriteBatch::default();
        access.delete_all(BatchDbWriter::new(&mut batch)).unwrap();
        assert_eq!(16, access.iterator().count());
        db.write(batch).unwrap();
        assert_eq!(0, access.iterator().count());
    }

    #[test]
    fn test_read_with_fallback() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let primary_prefix = vec![1];
        let fallback_prefix = vec![2];
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), primary_prefix);
        let fallback_access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), fallback_prefix.clone());

        let key: Hash = 1.into();
        let value = 100;

        // Write to fallback
        fallback_access.write(DirectDbWriter::new(&db), key, value).unwrap();

        // Read with fallback, should succeed
        let result = access.read_with_fallback::<u64>(&fallback_prefix, key).unwrap();
        assert_eq!(result, value);

        // Key should now be in the primary cache
        assert_eq!(access.read_from_cache(key).unwrap(), value);
    }

    #[test]
    fn test_has_with_fallback() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let primary_prefix = vec![1];
        let fallback_prefix = vec![2];
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), primary_prefix);
        let fallback_access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), fallback_prefix.clone());

        let key_in_fallback: Hash = 1.into();
        let key_not_found: Hash = 2.into();

        // Write to fallback
        fallback_access.write(DirectDbWriter::new(&db), key_in_fallback, 100).unwrap();

        // Check for key in fallback, should exist
        assert!(access.has_with_fallback(&fallback_prefix, key_in_fallback).unwrap());

        // Check for key that doesn't exist, should not be found
        assert!(!access.has_with_fallback(&fallback_prefix, key_not_found).unwrap());
    }

    mod versioned {
        //! Tests for the version-aware code path introduced for UtxoDiffs
        //! pre/post-Toccata compatibility. Each test uses a dedicated
        //! `Current` / `Legacy` pair so the legacy decode path is wired
        //! through a real `TLegacy: Into<TData>` conversion rather than
        //! relying on the trivial `T -> T` identity.
        use super::*;
        use crate::errors::StoreErrorPredicates;
        use serde::{Deserialize, Serialize};

        /// Post-fork value type.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Current {
            value: u64,
            extra: Option<u32>,
        }

        impl MemSizeEstimator for Current {}

        /// Pre-fork value type — one field shorter than `Current`. Its
        /// derived `Deserialize` consumes only the pre-fork byte layout
        /// and cleanly fails on the post-fork layout.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Legacy {
            value: u64,
        }

        impl From<Legacy> for Current {
            fn from(l: Legacy) -> Self {
                Current { value: l.value, extra: None }
            }
        }

        const STORE_PREFIX: u8 = 42;
        const VERSION_SUFFIX: u8 = 1;

        fn versioned_access(db: Arc<DB>) -> CachedDbAccess<Hash, Current, RandomState, Legacy> {
            CachedDbAccess::new_with_version_suffix(db, CachePolicy::Count(16), vec![STORE_PREFIX], VERSION_SUFFIX)
        }

        /// Builds the raw `[prefix || hash]` byte vector used by pre-fork writes.
        fn legacy_raw_key(hash: Hash) -> Vec<u8> {
            let mut bytes = vec![STORE_PREFIX];
            bytes.extend_from_slice(hash.as_bytes().as_ref());
            bytes
        }

        /// Builds the raw `[prefix || hash || suffix]` byte vector used by post-fork writes.
        fn versioned_raw_key(hash: Hash, suffix: u8) -> Vec<u8> {
            let mut bytes = legacy_raw_key(hash);
            bytes.push(suffix);
            bytes
        }

        #[test]
        fn versioned_round_trip() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 7.into();
            let value = Current { value: 1_000_000, extra: Some(42) };

            access.write(DirectDbWriter::new(&db), hash, value.clone()).unwrap();

            // On-disk bytes must live under the versioned key, NOT the legacy key.
            assert!(db.get_pinned(versioned_raw_key(hash, VERSION_SUFFIX)).unwrap().is_some());
            assert!(db.get_pinned(legacy_raw_key(hash)).unwrap().is_none());

            // Round trip through the access layer.
            assert_eq!(access.read(hash).unwrap(), value);

            // `has` sees the row.
            assert!(access.has(hash).unwrap());

            // Delete + re-read.
            access.delete(DirectDbWriter::new(&db), hash).unwrap();
            assert!(!access.has(hash).unwrap());
            assert!(access.read(hash).is_err());
        }

        #[test]
        fn versioned_read_decodes_legacy_row() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 13.into();
            let legacy = Legacy { value: 99 };
            let legacy_bytes = bincode::serialize(&legacy).unwrap();

            // Simulate a pre-fork writer: put raw bytes at the unversioned layout,
            // bypassing the access layer entirely.
            db.put(legacy_raw_key(hash), legacy_bytes).unwrap();

            // Reading through the version-aware access layer finds the legacy row
            // via the prefix scan, decodes it through `TLegacy`, and converts to
            // `Current` via `From<Legacy>`.
            let current = access.read(hash).unwrap();
            assert_eq!(current, Current { value: 99, extra: None });

            // `has` sees the legacy row too (enforces the "no dupe writes" guard).
            assert!(access.has(hash).unwrap());

            // Deleting through the access layer must clear the legacy row as well,
            // not just the (non-existent) versioned row.
            access.delete(DirectDbWriter::new(&db), hash).unwrap();
            assert!(db.get_pinned(legacy_raw_key(hash)).unwrap().is_none());
            assert!(!access.has(hash).unwrap());
        }

        #[test]
        fn versioned_read_rejects_unknown_suffix_byte() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 21.into();
            let bytes = bincode::serialize(&Current { value: 1, extra: None }).unwrap();

            // Write under an unknown version suffix — this binary does not
            // recognize it, so the read must refuse to decode.
            db.put(versioned_raw_key(hash, 0xAB), bytes).unwrap();

            let err = access.read(hash).unwrap_err();
            assert!(matches!(err, StoreError::DataInconsistency(_)), "expected DataInconsistency, got {err:?}");
        }

        #[test]
        fn versioned_read_rejects_unexpected_key_length() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 34.into();
            let bytes = bincode::serialize(&Current { value: 1, extra: None }).unwrap();

            // Corrupt on-disk layout: two trailing bytes after the logical key.
            let mut bogus_key = legacy_raw_key(hash);
            bogus_key.push(VERSION_SUFFIX);
            bogus_key.push(0x00);
            db.put(bogus_key, bytes).unwrap();

            let err = access.read(hash).unwrap_err();
            assert!(matches!(err, StoreError::DataInconsistency(_)), "expected DataInconsistency, got {err:?}");
        }

        #[test]
        fn versioned_delete_clears_both_layouts() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 55.into();

            // Plant both a legacy row AND a versioned row for the same logical key.
            // This scenario cannot arise through normal usage (the `insert` guard
            // refuses to overwrite), but the delete path must still be robust to
            // it so corrupted state can be cleaned up.
            db.put(legacy_raw_key(hash), bincode::serialize(&Legacy { value: 7 }).unwrap()).unwrap();
            db.put(versioned_raw_key(hash, VERSION_SUFFIX), bincode::serialize(&Current { value: 7, extra: Some(1) }).unwrap())
                .unwrap();

            access.delete(DirectDbWriter::new(&db), hash).unwrap();

            assert!(db.get_pinned(legacy_raw_key(hash)).unwrap().is_none());
            assert!(db.get_pinned(versioned_raw_key(hash, VERSION_SUFFIX)).unwrap().is_none());
        }

        #[test]
        fn versioned_has_guards_inserts_against_legacy_rows() {
            // This is the functional contract that keeps `read_versioned`'s
            // invariant (1) true: on a store that already holds a legacy row,
            // `has` must report true so that `insert_batch` / `insert` refuse
            // to write a second (versioned) row and shadow the legacy one.
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db.clone());

            let hash: Hash = 89.into();
            db.put(legacy_raw_key(hash), bincode::serialize(&Legacy { value: 11 }).unwrap()).unwrap();

            assert!(access.has(hash).unwrap(), "versioned has() must observe legacy rows");

            // Sanity: a hash with no row anywhere still reports false.
            assert!(!access.has(Hash::from_u64_word(12345)).unwrap());
        }

        #[test]
        fn versioned_read_from_missing_key_is_not_found() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let access = versioned_access(db);

            let err = access.read(Hash::from_u64_word(999)).unwrap_err();
            assert!(err.is_key_not_found(), "expected KeyNotFound, got {err:?}");
        }
    }
}
