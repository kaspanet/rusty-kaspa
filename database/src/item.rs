use crate::{
    db::DB,
    errors::StoreError,
    prelude::{DbSetAccess, ReadLock},
};

use super::prelude::{DbKey, DbWriter};
use parking_lot::RwLock;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{HashSet, hash_map::RandomState},
    hash::BuildHasher,
    marker::PhantomData,
    sync::Arc,
};

/// A cached DB item with concurrency support.
///
/// # Versioned items
///
/// `CachedDbItem` supports an opt-in "version-suffix" mode used by items
/// whose on-disk value layout changed across a hardfork (currently
/// `DbVirtualStateStore` across Toccata). A version-aware item is
/// constructed with [`CachedDbItem::new_with_version_suffix`].
///
/// The store keeps two precomputed physical keys:
/// - `live_key` — where the current-format row is read and written.
/// - `legacy_key` — where a legacy-format row, if any, lives. `None` for
///   non-versioned items.
///
/// On reads, the live key is probed first; on miss, the legacy key is
/// probed and decoded through a caller-provided `TLegacy: Into<T>` shadow
/// type. A successful legacy fallback eagerly migrates: the converted
/// bytes are written under `live_key` and `legacy_key` is deleted, so the
/// next read short-circuits on the live path and no dangling pre-fork
/// bytes linger on disk. `remove` clears both layouts.
///
/// Dispatch is on physical-key presence, not on bincode decode outcome,
/// so the version-aware path is robust to silent decode "successes"
/// against mismatched bytes.
///
/// The `TLegacy` generic defaults to `T`, so existing callers that spell
/// only the single type parameter continue to compile unchanged: `T:
/// Into<T>` is supplied by the blanket `impl<U> From<U> for U`.
pub struct CachedDbItem<T, TLegacy = T> {
    db: Arc<DB>,
    /// The physical DB key used by every read and write of the current
    /// row. For a non-versioned item this is just the caller's prefix
    /// bytes. For a versioned item it is `[caller_prefix || current_suffix]`.
    live_key: Vec<u8>,
    /// The physical DB key probed on a read miss. `Some` iff this item is
    /// version-aware. The unversioned form `[caller_prefix]` and the
    /// previously-versioned form `[caller_prefix || legacy_suffix]` are
    /// both expressible here — see [`Self::new_with_version_suffix`].
    legacy_key: Option<Vec<u8>>,
    cached_item: Arc<RwLock<Option<T>>>,
    _legacy: PhantomData<fn() -> TLegacy>,
}

// Manual `Clone` so the impl does NOT require `TLegacy: Clone` (the legacy
// type is only used at decode time and never stored). `#[derive(Clone)]`
// would conservatively bound every generic.
impl<T, TLegacy> Clone for CachedDbItem<T, TLegacy> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            live_key: self.live_key.clone(),
            legacy_key: self.legacy_key.clone(),
            cached_item: self.cached_item.clone(),
            _legacy: PhantomData,
        }
    }
}

impl<T, TLegacy> CachedDbItem<T, TLegacy> {
    pub fn new(db: Arc<DB>, key: Vec<u8>) -> Self {
        Self { db, live_key: key, legacy_key: None, cached_item: Arc::new(RwLock::new(None)), _legacy: PhantomData }
    }

    /// Constructs a version-aware item.
    ///
    /// Writes go to `[key_prefix || current_suffix]`. Reads probe that
    /// key first and, on miss, fall back to the legacy row decoded through
    /// `TLegacy` and converted via `TLegacy: Into<T>`. The legacy row's
    /// physical key is:
    /// - `[key_prefix]` when `legacy_suffix = None` (pre-versioning,
    ///   unversioned on-disk layout — the Toccata case);
    /// - `[key_prefix || s]` when `legacy_suffix = Some(s)` (a previously
    ///   versioned layout — placeholder for future migrations).
    ///
    /// A successful legacy fallback triggers an eager migration: the
    /// converted `T` is written under the live key and the legacy row is
    /// deleted. `remove` clears both layouts.
    pub fn new_with_version_suffix(db: Arc<DB>, key_prefix: Vec<u8>, current_suffix: u8, legacy_suffix: Option<u8>) -> Self {
        let mut live_key = key_prefix.clone();
        live_key.push(current_suffix);
        let legacy_key = match legacy_suffix {
            None => Some(key_prefix),
            Some(s) => {
                let mut k = key_prefix;
                k.push(s);
                Some(k)
            }
        };
        Self { db, live_key, legacy_key, cached_item: Arc::new(RwLock::new(None)), _legacy: PhantomData }
    }

    pub fn read(&self) -> Result<T, StoreError>
    where
        T: Clone + Serialize + DeserializeOwned,
        TLegacy: DeserializeOwned + Into<T>,
    {
        if let Some(item) = self.cached_item.read().clone() {
            return Ok(item);
        }
        if let Some(slice) = self.db.get_pinned(&self.live_key)? {
            let item: T = bincode::deserialize(&slice)?;
            *self.cached_item.write() = Some(item.clone());
            return Ok(item);
        }
        // No live row. Fall through to the legacy probe if version-aware,
        // else this is just KeyNotFound on the single physical key.
        if let Some(legacy_key) = self.legacy_key.as_deref()
            && let Some(slice) = self.db.get_pinned(legacy_key)?
        {
            let legacy_value: TLegacy = bincode::deserialize(&slice)?;
            let item: T = legacy_value.into();
            // Migrate: write under the live key and clear the legacy row.
            let bin_data = bincode::serialize(&item)?;
            self.db.put(&self.live_key, bin_data)?;
            self.db.delete(legacy_key)?;
            *self.cached_item.write() = Some(item.clone());
            return Ok(item);
        }
        Err(StoreError::KeyNotFound(DbKey::prefix_only(&self.live_key)))
    }

    pub fn write(&mut self, mut writer: impl DbWriter, item: &T) -> Result<(), StoreError>
    where
        T: Clone + Serialize,
    {
        *self.cached_item.write() = Some(item.clone());
        let bin_data = bincode::serialize(item)?;
        writer.put(&self.live_key, bin_data)?;
        Ok(())
    }

    pub fn remove(&mut self, mut writer: impl DbWriter) -> Result<(), StoreError>
where {
        *self.cached_item.write() = None;
        writer.delete(&self.live_key)?;
        if let Some(legacy_key) = self.legacy_key.as_deref() {
            writer.delete(legacy_key)?;
        }
        Ok(())
    }

    pub fn update<F>(&mut self, mut writer: impl DbWriter, op: F) -> Result<T, StoreError>
    where
        T: Clone + Serialize + DeserializeOwned,
        TLegacy: DeserializeOwned + Into<T>,
        F: Fn(T) -> T,
    {
        let mut guard = self.cached_item.write();
        let mut item = if let Some(item) = guard.take() {
            item
        } else if let Some(slice) = self.db.get_pinned(&self.live_key)? {
            bincode::deserialize(&slice)?
        } else if let Some(legacy_slice) = self.legacy_key.as_deref().and_then(|k| self.db.get_pinned(k).transpose()).transpose()? {
            let legacy_value: TLegacy = bincode::deserialize(&legacy_slice)?;
            legacy_value.into()
        } else {
            return Err(StoreError::KeyNotFound(DbKey::prefix_only(&self.live_key)));
        };

        item = op(item); // Apply the update op
        *guard = Some(item.clone());
        let bin_data = bincode::serialize(&item)?;
        writer.put(&self.live_key, bin_data)?;
        Ok(item)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct EmptyKey;

impl AsRef<[u8]> for EmptyKey {
    fn as_ref(&self) -> &[u8] {
        &[]
    }
}

type LockedSet<T, S> = Arc<RwLock<HashSet<T, S>>>;

#[derive(Clone)]
pub struct CachedDbSetItem<T: Clone + Send + Sync, S = RandomState> {
    access: DbSetAccess<EmptyKey, T>,
    cached_set: Arc<RwLock<Option<LockedSet<T, S>>>>,
}

impl<T, S> CachedDbSetItem<T, S>
where
    T: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, key: Vec<u8>) -> Self {
        Self { access: DbSetAccess::new(db, key), cached_set: Arc::new(RwLock::new(None)) }
    }

    fn read_locked_set(&self) -> Result<LockedSet<T, S>, StoreError>
    where
        T: Clone + DeserializeOwned,
    {
        if let Some(item) = self.cached_set.read().clone() {
            return Ok(item);
        }
        let set = self.access.bucket_iterator(EmptyKey).collect::<Result<HashSet<_, _>, _>>()?;
        let set = Arc::new(RwLock::new(set));
        self.cached_set.write().replace(set.clone());
        Ok(set)
    }

    pub fn read(&self) -> Result<ReadLock<HashSet<T, S>>, StoreError>
    where
        T: Clone + DeserializeOwned,
    {
        Ok(ReadLock::new(self.read_locked_set()?))
    }

    pub fn update(
        &mut self,
        mut writer: impl DbWriter,
        added_items: &[T],
        removed_items: &[T],
    ) -> Result<ReadLock<HashSet<T, S>>, StoreError>
    where
        T: Clone + Serialize,
    {
        let set = self.read_locked_set()?;
        {
            let mut set_write = set.write();
            for item in removed_items.iter() {
                self.access.delete(&mut writer, EmptyKey, item.clone())?;
                set_write.remove(item);
            }
            for item in added_items.iter().cloned() {
                self.access.write(&mut writer, EmptyKey, item.clone())?;
                set_write.insert(item);
            }
        }
        Ok(ReadLock::new(set))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_temp_db,
        errors::StoreErrorPredicates,
        prelude::{ConnBuilder, DirectDbWriter},
    };
    use serde::Deserialize;

    /// Tests for the version-aware code path on `CachedDbItem`, mirroring
    /// the `CachedDbAccess` versioned tests added in PR #956 for
    /// `DbUtxoDiffsStore`. Each test uses a dedicated `Current` / `Legacy`
    /// pair so the legacy decode path is exercised through a real
    /// `TLegacy: Into<T>` conversion rather than the trivial `T -> T`
    /// identity.
    mod versioned {
        use super::*;

        /// Post-fork value type.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Current {
            value: u64,
            extra: Option<u32>,
        }

        /// Pre-fork value type — one field shorter than `Current`. Its
        /// derived `Deserialize` consumes only the pre-fork byte layout.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Legacy {
            value: u64,
        }

        impl From<Legacy> for Current {
            fn from(l: Legacy) -> Self {
                Current { value: l.value, extra: None }
            }
        }

        const ITEM_KEY: &[u8] = &[42];
        const VERSION_SUFFIX: u8 = 1;

        fn versioned_item(db: Arc<DB>) -> CachedDbItem<Current, Legacy> {
            // `legacy_suffix = None` ⇒ the legacy row is the unversioned
            // `[key]` layout (the Toccata case). The `Some(_)` form is
            // exercised by `versioned_to_versioned_migration` below.
            CachedDbItem::new_with_version_suffix(db, ITEM_KEY.to_vec(), VERSION_SUFFIX, None)
        }

        fn versioned_raw_key() -> Vec<u8> {
            let mut k = ITEM_KEY.to_vec();
            k.push(VERSION_SUFFIX);
            k
        }

        #[test]
        fn versioned_round_trip() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let mut item = versioned_item(db.clone());

            let value = Current { value: 1_000_000, extra: Some(42) };
            item.write(DirectDbWriter::new(&db), &value).unwrap();

            // On-disk bytes must live under the versioned key, NOT the legacy key.
            assert!(db.get_pinned(versioned_raw_key()).unwrap().is_some());
            assert!(db.get_pinned(ITEM_KEY).unwrap().is_none());

            // Round trip — fresh item (no cache) to force a DB hit.
            let fresh = versioned_item(db.clone());
            assert_eq!(fresh.read().unwrap(), value);
        }

        #[test]
        fn versioned_read_migrates_legacy_row() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let item = versioned_item(db.clone());

            let legacy = Legacy { value: 99 };
            db.put(ITEM_KEY, bincode::serialize(&legacy).unwrap()).unwrap();

            // Reading through the version-aware item finds the legacy row at
            // `[key]`, decodes via `Legacy`, converts into `Current`, and
            // migrates the on-disk layout to the versioned key.
            let current = item.read().unwrap();
            assert_eq!(current, Current { value: 99, extra: None });

            // Post-read: the legacy row is gone and the versioned row holds
            // the converted bytes.
            assert!(db.get_pinned(ITEM_KEY).unwrap().is_none(), "legacy row must be deleted after migration");
            let migrated: Current = {
                let migrated_bytes = db.get_pinned(versioned_raw_key()).unwrap().expect("versioned row must exist after migration");
                bincode::deserialize(&migrated_bytes).unwrap()
            };
            assert_eq!(migrated, Current { value: 99, extra: None });

            // A fresh item (no cache) sees the migrated row directly.
            let fresh = versioned_item(db);
            assert_eq!(fresh.read().unwrap(), Current { value: 99, extra: None });
        }

        #[test]
        fn versioned_prefers_current_over_legacy_when_both_present() {
            // Cannot arise through normal usage (write never touches the legacy
            // key), but the read path must still deterministically return the
            // current row when an upgrade-time legacy row coexists.
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let mut item = versioned_item(db.clone());

            db.put(ITEM_KEY, bincode::serialize(&Legacy { value: 7 }).unwrap()).unwrap();
            item.write(DirectDbWriter::new(&db), &Current { value: 7, extra: Some(1) }).unwrap();

            let fresh = versioned_item(db.clone());
            assert_eq!(fresh.read().unwrap(), Current { value: 7, extra: Some(1) });
            // The current row is preferred, so the legacy row is NOT touched
            // — it lingers until the next `remove`. (Crash-recovery scenario.)
            assert!(db.get_pinned(ITEM_KEY).unwrap().is_some());
        }

        #[test]
        fn versioned_remove_clears_both_layouts() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let mut item = versioned_item(db.clone());

            // Plant both a legacy row AND a current row for the same logical key.
            db.put(ITEM_KEY, bincode::serialize(&Legacy { value: 7 }).unwrap()).unwrap();
            item.write(DirectDbWriter::new(&db), &Current { value: 7, extra: Some(1) }).unwrap();

            item.remove(DirectDbWriter::new(&db)).unwrap();

            assert!(db.get_pinned(ITEM_KEY).unwrap().is_none());
            assert!(db.get_pinned(versioned_raw_key()).unwrap().is_none());
            assert!(item.read().unwrap_err().is_key_not_found());
        }

        #[test]
        fn versioned_read_from_missing_key_is_not_found() {
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let item = versioned_item(db);
            assert!(item.read().unwrap_err().is_key_not_found());
        }

        /// Exercises the `legacy_suffix = Some(_)` branch — the future
        /// "vN → vN+1" rotation. Plants a row under `[key || PREV]`,
        /// reads through an item configured for `current = NEXT, legacy =
        /// Some(PREV)`, and asserts the migration rewrites the row under
        /// `[key || NEXT]` and clears `[key || PREV]`.
        #[test]
        fn versioned_to_versioned_migration() {
            const PREV: u8 = 1;
            const NEXT: u8 = 2;
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

            let item: CachedDbItem<Current, Legacy> =
                CachedDbItem::new_with_version_suffix(db.clone(), ITEM_KEY.to_vec(), NEXT, Some(PREV));

            let mut prev_key = ITEM_KEY.to_vec();
            prev_key.push(PREV);
            let mut next_key = ITEM_KEY.to_vec();
            next_key.push(NEXT);

            db.put(&prev_key, bincode::serialize(&Legacy { value: 314 }).unwrap()).unwrap();

            let current = item.read().unwrap();
            assert_eq!(current, Current { value: 314, extra: None });

            assert!(db.get_pinned(&prev_key).unwrap().is_none(), "previous-version row must be deleted");
            assert!(db.get_pinned(&next_key).unwrap().is_some(), "next-version row must exist");
            // The unversioned `[key]` slot was never touched — only the
            // configured legacy key is migrated, not all older variants.
            assert!(db.get_pinned(ITEM_KEY).unwrap().is_none());
        }

        #[test]
        fn non_versioned_default_unchanged() {
            // Sanity: a default `CachedDbItem::new` keeps the unversioned key
            // layout — the version-aware extension is opt-in only.
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let mut item: CachedDbItem<Current> = CachedDbItem::new(db.clone(), ITEM_KEY.to_vec());
            let value = Current { value: 5, extra: None };
            item.write(DirectDbWriter::new(&db), &value).unwrap();
            assert!(db.get_pinned(ITEM_KEY).unwrap().is_some());
            assert!(db.get_pinned(versioned_raw_key()).unwrap().is_none());
        }
    }
}
