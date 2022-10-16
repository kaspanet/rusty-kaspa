use super::{
    caching::{BatchDbWriter, CachedDbAccess},
    errors::{StoreError, StoreResultExtensions},
    DB,
};
use consensus_core::{
    tx::{TransactionIndexType, TransactionOutpoint, UtxoEntry},
    utxo::{
        utxo_diff::{ImmutableUtxoDiff, UtxoDiff},
        utxo_view::UtxoView,
    },
};
use hashes::Hash;
use rocksdb::WriteBatch;
use std::{fmt::Display, sync::Arc};

pub trait UtxoSetStoreReader {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<UtxoEntry>, StoreError>;
    // TODO: UTXO entry iterator
}

pub trait UtxoSetStore: UtxoSetStoreReader {
    fn write_entry(&self, outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> Result<(), StoreError>;
    fn write_diff(&self, utxo_diff: &UtxoDiff) -> Result<(), StoreError>;
}

pub const UTXO_KEY_SIZE: usize = hashes::HASH_SIZE + std::mem::size_of::<TransactionIndexType>();

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct UtxoKey([u8; UTXO_KEY_SIZE]);

impl AsRef<[u8]> for UtxoKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
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
        bytes[..hashes::HASH_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[hashes::HASH_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}

impl From<UtxoKey> for TransactionOutpoint {
    fn from(k: UtxoKey) -> Self {
        let transaction_id = Hash::from_slice(&k.0[..hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; std::mem::size_of::<TransactionIndexType>()]>::try_from(&k.0[hashes::HASH_SIZE..]).expect("expecting index size"),
        );
        Self::new(transaction_id, index)
    }
}

#[derive(Clone)]
pub struct DbUtxoSetStore {
    raw_db: Arc<DB>,
    prefix: &'static [u8],
    cached_access: CachedDbAccess<UtxoKey, UtxoEntry>,
}

impl DbUtxoSetStore {
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: &'static [u8]) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size, prefix), prefix }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size, self.prefix)
    }

    pub fn write_entry_batch(
        &self,
        batch: &mut WriteBatch,
        outpoint: &TransactionOutpoint,
        entry: &UtxoEntry,
    ) -> Result<(), StoreError> {
        todo!()
    }

    pub fn write_diff_batch(&self, batch: &mut WriteBatch, utxo_diff: &impl ImmutableUtxoDiff) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        self.cached_access.delete_many(&mut writer, &mut utxo_diff.removed().keys().map(|o| (*o).into()))?;
        self.cached_access.write_many(&mut writer, &mut utxo_diff.added().iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
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
        self.cached_access.read((*outpoint).into())
    }
}

impl UtxoSetStore for DbUtxoSetStore {
    fn write_entry(&self, outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> Result<(), StoreError> {
        todo!()
    }

    fn write_diff(&self, utxo_diff: &UtxoDiff) -> Result<(), StoreError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utxo_key_conversion() {
        let outpoint = TransactionOutpoint::new(234.into(), 17);
        let key: UtxoKey = outpoint.into();
        // println!("{}, {}", outpoint, key);
        assert_eq!(outpoint, key.into());
    }
}
