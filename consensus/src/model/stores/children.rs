use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::BlockLevel;
use kaspa_database::prelude::DbWriter;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_hashes::HASH_SIZE;
use rocksdb::WriteBatch;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

pub trait ChildrenStoreReader {
    fn get(&self, hash: Hash) -> Result<Vec<Hash>, Box<dyn Error>>;
}

pub trait ChildrenStore {
    fn insert_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
    fn delete_children(&self, writer: impl DbWriter, parent: Hash) -> Result<(), StoreError>;
    fn delete_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError>;
}

struct ChildKey {
    parent: Hash,
    child: Hash,
}

const KEY_SIZE: usize = 2 * HASH_SIZE;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct DbChildKey([u8; KEY_SIZE]);

impl AsRef<[u8]> for DbChildKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for DbChildKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key: ChildKey = (*self).into();
        write!(f, "{}:{}", key.parent, key.child)
    }
}

impl From<ChildKey> for DbChildKey {
    fn from(key: ChildKey) -> Self {
        let mut bytes = [0; KEY_SIZE];
        bytes[..HASH_SIZE].copy_from_slice(&key.parent.as_bytes());
        bytes[HASH_SIZE..].copy_from_slice(&key.child.as_bytes());
        Self(bytes)
    }
}

impl From<DbChildKey> for ChildKey {
    fn from(k: DbChildKey) -> Self {
        let parent_bytes: [u8; HASH_SIZE] = k.0[..HASH_SIZE].try_into().unwrap();
        let parent: Hash = parent_bytes.into();

        let child_bytes: [u8; HASH_SIZE] = k.0[HASH_SIZE..].try_into().unwrap();
        let child: Hash = child_bytes.into();
        Self { parent, child }
    }
}

/// A DB + cache implementation of `DbChildrenStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbChildrenStore {
    db: Arc<DB>,
    access: CachedDbAccess<DbChildKey, (), BlockHasher>,
}

impl DbChildrenStore {
    pub fn new(db: Arc<DB>, level: BlockLevel) -> Self {
        let lvl_bytes = level.to_le_bytes();
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, 0, DatabaseStorePrefixes::RelationsChildren.into_iter().chain(lvl_bytes).collect()),
        }
    }

    pub fn with_prefix(db: Arc<DB>, prefix: &[u8]) -> Self {
        let db_prefix = prefix.iter().copied().chain(DatabaseStorePrefixes::RelationsChildren).collect();
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, 0, db_prefix) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let key = ChildKey { parent, child }.into();
        if self.access.has(key)? {
            return Err(StoreError::HashAlreadyExists(parent));
        }
        self.access.write(BatchDbWriter::new(batch), key, ())?;
        Ok(())
    }

    fn iterator(&self, parent: Hash) -> impl Iterator<Item = Result<Hash, Box<dyn Error>>> + '_ {
        self.access.seek_iterator(Some(parent.as_bytes().as_ref()), None, usize::MAX, false).map(|res| {
            let (key, _) = res.unwrap();
            match Hash::try_from(&key[..]) {
                Ok(hash) => Ok(hash),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn count(&self) -> Result<usize, StoreError> {
        Ok(self.access.iterator().count())
    }

    // pub fn delete_batch(&self, batch: &mut WriteBatch, parent: Hash) -> Result<(), StoreError> {
    //     self.access.delete(BatchDbWriter::new(batch), parent)
    // }
}

impl ChildrenStoreReader for DbChildrenStore {
    fn get(&self, parent: Hash) -> Result<Vec<Hash>, Box<dyn Error>> {
        // TODO: Cache the whole result
        self.iterator(parent).collect()
    }
}

impl ChildrenStore for DbChildrenStore {
    fn insert_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        let key = ChildKey { parent, child }.into();
        if self.access.has(key)? {
            return Err(StoreError::KeyAlreadyExists(key.to_string()));
        }
        self.access.write(writer, key, ())?;
        Ok(())
    }

    fn delete_children(&self, mut writer: impl DbWriter, parent: Hash) -> Result<(), StoreError> {
        for child in self.get(parent).unwrap() {
            self.access.delete(&mut writer, ChildKey { parent, child }.into())?;
        }

        Ok(())
    }

    fn delete_child(&self, writer: impl DbWriter, parent: Hash, child: Hash) -> Result<(), StoreError> {
        self.access.delete(writer, ChildKey { parent, child }.into())
    }
}
