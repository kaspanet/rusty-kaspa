use kaspa_utils::refs::Refs;
use rocksdb::WriteBatch;

use crate::prelude::DB;

/// Abstraction over direct/batched DB writing
pub trait DbWriter {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error>;
    fn delete_range<K>(&mut self, from: K, to: K) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>;
}

/// A trait which is intentionally not implemented for the batch writer.
/// Aimed for compile-time safety of operations which do not support batch writing semantics
pub trait DirectWriter: DbWriter {}

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

    fn delete_range<K>(&mut self, from: K, to: K) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
    {
        let mut batch = WriteBatch::default();
        batch.delete_range(from, to);
        self.db.write(batch)
    }
}

impl DirectWriter for DirectDbWriter<'_> {}

pub struct BatchDbWriter<'a> {
    batch: &'a mut WriteBatch,
}

impl<'a> BatchDbWriter<'a> {
    pub fn new(batch: &'a mut WriteBatch) -> Self {
        Self { batch }
    }
}

impl DbWriter for BatchDbWriter<'_> {
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

    fn delete_range<K>(&mut self, from: K, to: K) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
    {
        self.batch.delete_range(from, to);
        Ok(())
    }
}

impl<T: DbWriter> DbWriter for &mut T {
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

    #[inline]
    fn delete_range<K>(&mut self, from: K, to: K) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).delete_range(from, to)
    }
}

impl<T: DirectWriter> DirectWriter for &mut T {}

/// A writer for memory stores which writes nothing to the DB
#[derive(Default)]
pub struct MemoryWriter;

impl DbWriter for MemoryWriter {
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

    fn delete_range<K>(&mut self, _from: K, _to: K) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(())
    }
}

impl DirectWriter for MemoryWriter {}
