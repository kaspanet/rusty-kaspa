use kaspa_consensus_core::{
    tx::{TransactionIndexType, TransactionOutpoint, UtxoEntry},
    utxo::{
        utxo_diff::{ImmutableUtxoDiff, UtxoDiff},
        utxo_view::UtxoView,
    },
};
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, StoreError};
use kaspa_database::prelude::{CachedDbItem, StoreResultExtensions};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::{error::Error, fmt::Display, sync::Arc};

type UtxoCollectionIterator<'a> = Box<dyn Iterator<Item = Result<(TransactionOutpoint, UtxoEntry), Box<dyn Error>>> + 'a>;

pub trait UtxoSetStoreReader {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<UtxoEntry>, StoreError>;
    fn seek_iterator(&self, from_outpoint: Option<TransactionOutpoint>, limit: usize, skip_first: bool) -> UtxoCollectionIterator;
    fn count(&self) -> Result<u64, StoreError>;
}

pub trait UtxoSetStore: UtxoSetStoreReader {
    /// Updates the store according to the UTXO diff -- adding and deleting entries correspondingly.
    /// Note we define `self` as `mut` in order to require write access even though the compiler does not require it.
    /// This is because concurrent readers can interfere with cache consistency.  
    fn write_diff(&mut self, utxo_diff: &UtxoDiff) -> Result<(), StoreError>;
    fn write_many(&mut self, utxos: &[(TransactionOutpoint, UtxoEntry)]) -> Result<(), StoreError>;
}

pub const UTXO_KEY_SIZE: usize = kaspa_hashes::HASH_SIZE + std::mem::size_of::<TransactionIndexType>();

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct UtxoKey([u8; UTXO_KEY_SIZE]);

impl AsRef<[u8]> for UtxoKey {
    fn as_ref(&self) -> &[u8] {
        // In every practical case a transaction output index needs at most 2 bytes, so the overall
        // DB key structure will be { prefix byte || TX ID (32 bytes) || TX INDEX (2) } = 35 bytes
        // which fit on the smallvec without requiring heap allocation (see key.rs)
        let rposition = self.0[kaspa_hashes::HASH_SIZE..].iter().rposition(|&v| v != 0).unwrap_or(0);
        &self.0[..=kaspa_hashes::HASH_SIZE + rposition]
    }
}

impl TryFrom<&[u8]> for UtxoKey {
    type Error = &'static str;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        if UTXO_KEY_SIZE < slice.len() {
            return Err("src slice is too large");
        }
        if slice.len() < kaspa_hashes::HASH_SIZE + 1 {
            return Err("src slice is too short");
        }
        // If the slice is shorter than HASH len + u32 len then we pad with zeros, effectively
        // implementing the inverse logic of `AsRef`.
        let mut bytes = [0; UTXO_KEY_SIZE];
        bytes[..slice.len()].copy_from_slice(slice);
        Ok(Self(bytes))
    }
}

impl Display for UtxoKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let outpoint: TransactionOutpoint = (*self).into();
        outpoint.fmt(f)
    }
}

impl From<TransactionOutpoint> for UtxoKey {
    fn from(outpoint: TransactionOutpoint) -> Self {
        let mut bytes = [0; UTXO_KEY_SIZE];
        bytes[..kaspa_hashes::HASH_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[kaspa_hashes::HASH_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}

impl From<UtxoKey> for TransactionOutpoint {
    fn from(k: UtxoKey) -> Self {
        let transaction_id = Hash::from_slice(&k.0[..kaspa_hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; std::mem::size_of::<TransactionIndexType>()]>::try_from(&k.0[kaspa_hashes::HASH_SIZE..])
                .expect("expecting index size"),
        );
        Self::new(transaction_id, index)
    }
}

#[derive(Clone)]
pub struct DbUtxoSetStore {
    db: Arc<DB>,
    // Prefixes
    prefix: Vec<u8>,
    count_prefix: Vec<u8>,
    // Accesses
    access: CachedDbAccess<UtxoKey, Arc<UtxoEntry>>,
    // TODO: implement CachedAtomicDbItem store for such primitives.
    // Should be no need to use a RwLock implicitly here.
    count: CachedDbItem<u64>,
}

impl DbUtxoSetStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>, count_prefix: Vec<u8>) -> Self {
        let mut ret = Self {
            db: db.clone(),
            prefix: prefix.clone(),
            access: CachedDbAccess::new(db.clone(), cache_policy, prefix),
            count_prefix: count_prefix.clone(),
            count: CachedDbItem::new(db.clone(), count_prefix.clone()),
        };

        // TODO: remove this code in the next Hard Fork effort
        // This does a one off count of the db for nodes running with the count feature.
        match ret.count.read() {
            Ok(_) => (),
            Err(err) => match err {
                StoreError::KeyNotFound(_) => {
                    let mut writer = DirectDbWriter::new(&db);
                    ret.count.write(&mut writer, &(ret.access.iterator().count() as u64)).unwrap();
                }
                _ => panic!("Unexpected Error reading count from db {0}: {1}", *count_prefix.first().unwrap(), err),
            },
        };

        ret
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy, self.prefix.clone(), self.count_prefix.clone())
    }

    /// See comment at [`UtxoSetStore::write_diff`]
    pub fn write_diff_batch(&mut self, batch: &mut WriteBatch, utxo_diff: &impl ImmutableUtxoDiff) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        self.count.update(&mut writer, |count| (count + utxo_diff.added().len() as u64) - utxo_diff.removed().len() as u64)?;
        self.access.delete_many(&mut writer, &mut utxo_diff.removed().keys().map(|o| (*o).into()))?;
        self.access.write_many(&mut writer, &mut utxo_diff.added().iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
        Ok(())
    }

    pub fn iterator(&self) -> impl Iterator<Item = Result<(TransactionOutpoint, Arc<UtxoEntry>), Box<dyn Error>>> + '_ {
        //TODO: exact size iterator should be implementable if we have a way to utilize the count.
        self.access.iterator().map(|iter_result| match iter_result {
            Ok((key_bytes, utxo_entry)) => match UtxoKey::try_from(key_bytes.as_ref()) {
                Ok(utxo_key) => {
                    let outpoint: TransactionOutpoint = utxo_key.into();
                    Ok((outpoint, utxo_entry))
                }
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e),
        })
    }

    /// Clear the store completely in DB and cache
    pub fn clear(&mut self) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db); // batch instead?
        self.access.delete_all(&mut writer)?;
        self.count.write(&mut writer, &0u64)
    }

    /// Write directly from an iterator and do not cache any data. NOTE: this action also clears the cache
    pub fn write_from_iterator_without_cache(
        &mut self,
        utxos: impl IntoIterator<Item = (TransactionOutpoint, Arc<UtxoEntry>)>,
    ) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db); //  batch instead?
        let mut count = 0u64;
        self.access.write_many_without_cache(
            &mut writer,
            &mut utxos.into_iter().map(|(o, e)| {
                count += 1;
                (o.into(), e)
            }),
        )?;
        self.count.update(&mut writer, |c| c + count)?;
        Ok(())
    }
}

impl UtxoView for DbUtxoSetStore {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<UtxoEntry> {
        UtxoSetStoreReader::get(self, outpoint).map(|v| v.as_ref().clone()).unwrap_option()
    }
}

impl UtxoSetStoreReader for DbUtxoSetStore {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<UtxoEntry>, StoreError> {
        self.access.read((*outpoint).into())
    }

    fn seek_iterator(&self, from_outpoint: Option<TransactionOutpoint>, limit: usize, skip_first: bool) -> UtxoCollectionIterator {
        let seek_key = from_outpoint.map(UtxoKey::from);
        Box::new(self.access.seek_iterator(None, seek_key, limit, skip_first).map(|res| {
            let (key, entry) = res?;
            let outpoint: TransactionOutpoint = UtxoKey::try_from(key.as_ref()).unwrap().into();
            Ok((outpoint, UtxoEntry::clone(&entry)))
        }))
    }

    fn count(&self) -> Result<u64, StoreError> {
        self.count.read()
    }
}

impl UtxoSetStore for DbUtxoSetStore {
    fn write_diff(&mut self, utxo_diff: &UtxoDiff) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.count.update(&mut writer, |count| count + utxo_diff.added().len() as u64 - utxo_diff.removed().len() as u64)?;
        self.access.delete_many(&mut writer, &mut utxo_diff.removed().keys().map(|o| (*o).into()))?;
        self.access.write_many(&mut writer, &mut utxo_diff.added().iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
        Ok(())
    }

    fn write_many(&mut self, utxos: &[(TransactionOutpoint, UtxoEntry)]) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.count.update(&mut writer, |count| count + utxos.len() as u64)?;
        self.access.write_many(&mut writer, &mut utxos.iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::test_helpers::{generate_random_outpoint, generate_random_utxo};

    use super::*;
    use itertools::Itertools;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder, registry::DatabaseStorePrefixes};
    use rand::{rngs::SmallRng, SeedableRng};

    #[test]
    fn test_utxo_key_conversion() {
        let tx_id = 2345.into();
        [300u32, 1, u8::MAX as u32, u16::MAX as u32, u32::MAX - 10].into_iter().for_each(|index| {
            let outpoint = TransactionOutpoint::new(tx_id, index);
            let key: UtxoKey = outpoint.into();
            let bytes = key.as_ref().to_vec();
            assert_eq!(key, bytes.as_slice().try_into().unwrap());
            assert_eq!(outpoint, key.into());
            assert_eq!(key.0.to_vec(), tx_id.as_bytes().iter().copied().chain(index.to_le_bytes().iter().copied()).collect_vec());
        });
    }

    #[test]
    fn test_count() {
        let (_db_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetStore::new(
            db.clone(),
            CachePolicy::Empty,
            DatabaseStorePrefixes::VirtualUtxoset.into(),
            DatabaseStorePrefixes::VirtualUtxosetCount.into(),
        );
        let mut rng: SmallRng = SmallRng::seed_from_u64(42u64);
        // test added only
        let to_add = (0..2).map(|_| (generate_random_outpoint(&mut rng), generate_random_utxo(&mut rng))).collect();
        let mut utxo_diff = UtxoDiff::new(to_add, HashMap::new());

        // Test write_diff
        // write 2
        let mut batch = WriteBatch::default();
        store.write_diff_batch(&mut batch, &utxo_diff).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 2);

        // Write 2 & Remove 2
        utxo_diff.add.iter().take(2).for_each(|(o, v)| {
            utxo_diff.remove.insert(*o, v.clone());
        });
        utxo_diff.add.clear();
        utxo_diff.add = (0..2).map(|_| (generate_random_outpoint(&mut rng), generate_random_utxo(&mut rng))).collect();

        let mut batch = WriteBatch::default();
        store.write_diff_batch(&mut batch, &utxo_diff).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 2);
        utxo_diff.remove.clear();
        // Remove 2

        let mut batch = WriteBatch::default();
        utxo_diff.add.iter().take(2).for_each(|(o, v)| {
            utxo_diff.remove.insert(*o, v.clone());
        });
        utxo_diff.add.clear();
        store.write_diff_batch(&mut batch, &utxo_diff).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 0);
        utxo_diff.remove.clear();

        // Test write_many
        // Write 2
        utxo_diff.add = (0..2).map(|_| (generate_random_outpoint(&mut rng), generate_random_utxo(&mut rng).into())).collect();
        store.write_many(&utxo_diff.add.iter().map(|(o, v)| (*o, v.clone())).collect_vec()).unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 2);
        utxo_diff.add.clear();

        // Test Iterator
        // write 2
        store
            .write_from_iterator_without_cache(
                (0..2).map(|_| (generate_random_outpoint(&mut rng), Arc::new(generate_random_utxo(&mut rng)))),
            )
            .unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 4);

        // Test clear
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), store.iterator().count() as u64);
        assert_eq!(store.count().unwrap(), 0);
    }
}
