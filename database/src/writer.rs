use rocksdb::WriteBatch;

use crate::prelude::DB;

/// Abstraction over direct/batched DB writing
pub trait DbWriter {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error>;
}

pub struct DirectDbWriter<'a> {
    db: &'a DB,
}

impl<'a> DirectDbWriter<'a> {
    pub fn new(db: &'a DB) -> Self {
        Self { db }
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
