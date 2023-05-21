use kaspa_utils::refs::Refs;
use rocksdb::WriteBatch;

use crate::prelude::DB;

/// Abstraction over direct/batched DB writing
pub trait DbWriter {
    const IS_BATCH: bool;

    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error>;
}

pub struct DirectDbWriter<'a> {
    db: Refs<'a, DB>,
}

impl<'a> DirectDbWriter<'a> {
    pub fn new(db: &'a DB) -> Self {
        Self { db: db.into() }
    }

    pub fn from_arc(db: std::sync::Arc<DB>) -> Self {
        Self { db: db.into() }
    }
}

impl DbWriter for DirectDbWriter<'_> {
    const IS_BATCH: bool = false;

    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db.put(key, value)
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error> {
        self.db.delete(key)
    }
}

pub struct BatchDbWriter<'a> {
    batch: &'a mut WriteBatch,
}

impl<'a> BatchDbWriter<'a> {
    pub fn new(batch: &'a mut WriteBatch) -> Self {
        Self { batch }
    }
}

impl DbWriter for BatchDbWriter<'_> {
    const IS_BATCH: bool = true;

    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.batch.put(key, value);
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error> {
        self.batch.delete(key);
        Ok(())
    }
}

impl<T: DbWriter> DbWriter for &mut T {
    const IS_BATCH: bool = T::IS_BATCH;

    #[inline]
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        (*self).put(key, value)
    }

    #[inline]
    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error> {
        (*self).delete(key)
    }
}

/// A writer for memory stores which writes nothing to the DB
#[derive(Default)]
pub struct MemoryWriter;

impl DbWriter for MemoryWriter {
    const IS_BATCH: bool = false;

    fn put<K, V>(&mut self, _key: K, _value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, _key: K) -> Result<(), rocksdb::Error> {
        Ok(())
    }
}
