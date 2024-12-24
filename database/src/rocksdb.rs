use crate::access::DbAccess;
use crate::errors::StoreError;
use crate::key::DbKey;
use crate::prelude::DbWriter;
pub use conn_builder::ConnBuilder;
use itertools::Either;
use kaspa_utils::fd_budget::FDGuard;
use rocksdb::{DBWithThreadMode, Direction, IterateBounds, IteratorMode, MultiThreaded, ReadOptions};
use std::borrow::Borrow;
use std::error::Error;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

mod conn_builder;

/// The DB type used for Kaspad stores
pub struct RocksDB {
    inner: DBWithThreadMode<MultiThreaded>,
    _fd_guard: FDGuard,
}

impl RocksDB {
    pub fn new(inner: DBWithThreadMode<MultiThreaded>, fd_guard: FDGuard) -> Self {
        Self { inner, _fd_guard: fd_guard }
    }
}

impl DerefMut for RocksDB {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Deref for RocksDB {
    type Target = DBWithThreadMode<MultiThreaded>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Deletes an existing DB if it exists
pub fn delete_db(db_dir: PathBuf) {
    if !db_dir.exists() {
        return;
    }
    let options = rocksdb::Options::default();
    let path = db_dir.to_str().unwrap();
    <DBWithThreadMode<MultiThreaded>>::destroy(&options, path).expect("DB is expected to be deletable");
}

impl<T: Borrow<RocksDB>> DbAccess for T {
    fn has(&self, db_key: DbKey) -> Result<bool, StoreError> {
        Ok(self.borrow().get_pinned(db_key)?.is_some())
    }

    fn read(&self, db_key: &DbKey) -> Result<Option<impl AsRef<[u8]>>, StoreError> {
        Ok(self.borrow().get_pinned(db_key)?)
    }

    fn iterator(
        &self,
        prefix: impl Into<Vec<u8>>,
        seek_from: Option<DbKey>,
    ) -> impl Iterator<Item = Result<(impl AsRef<[u8]>, impl AsRef<[u8]>), Box<dyn Error>>> + '_ {
        let prefix = prefix.into();
        seek_from.as_ref().inspect(|seek_from| debug_assert!(seek_from.as_ref().starts_with(prefix.as_ref())));
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(prefix));
        Iterator::map(
            {
                if let Some(seek_from) = seek_from {
                    Either::Left(self.borrow().iterator_opt(IteratorMode::From(seek_from.as_ref(), Direction::Forward), read_opts))
                } else {
                    Either::Right(self.borrow().iterator_opt(IteratorMode::Start, read_opts))
                }
            },
            |r| r.map_err(Into::into),
        )
    }

    fn write(&self, writer: &mut impl DbWriter, db_key: DbKey, data: Vec<u8>) -> Result<(), StoreError> {
        Ok(writer.put(db_key, data)?)
    }

    fn delete(&self, writer: &mut impl DbWriter, db_key: DbKey) -> Result<(), StoreError> {
        Ok(writer.delete(db_key)?)
    }

    fn delete_range_by_prefix(&self, writer: &mut impl DbWriter, prefix: &[u8]) -> Result<(), StoreError> {
        let (from, to) = rocksdb::PrefixRange(prefix).into_bounds();
        writer.delete_range(from.unwrap(), to.unwrap())?;
        Ok(())
    }
}
