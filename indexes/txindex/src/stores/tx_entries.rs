use kaspa_consensus::testutils::generate::from_rand::tx;
use kaspa_index_core::models::txindex::{TxHashSet, TxIndexEntry, TxIndexTxEntry, TxIndexTxEntryDiff, TxOffset, TxOffsetDiff};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use rocksdb::WriteBatch;
use std::{error::Error, sync::Arc};


pub type TxIndexTxEntriesIterator<'a> = Box<dyn Iterator<Item = Result<(TransactionId, TxIndexTxEntry), Box<dyn Error>>> + 'a>;

// Traits:
pub trait TxIndexTxEntriesReader {
    /// Get [`TransactionOffset`] queried by [`TransactionId`],
    fn get(&self, transaction_id: TransactionId) -> StoreResult<Option<TxIndexTxEntry>>;
    fn has(&self, transaction_id: TransactionId) -> StoreResult<bool>;
    fn seek_iterator(&self, from_transaction: Option<TransactionId>, limit: usize, skip_first: bool) -> TxIndexTxEntriesIterator;
    fn num_of_entries(&self) -> StoreResult<u64>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    // for this purpose, use `num_of_entries` instead.
    fn count(&self) -> StoreResult<usize>;
}

pub trait TxIndexTxEntriesStore: TxIndexTxEntriesReader {
    fn write_diff_batch(&mut self, batch: &mut WriteBatch, tx_offset_changes: TxIndexTxEntryDiff) -> StoreResult<()>;
    fn remove_many(&mut self, batch: &mut WriteBatch, tx_offsets_to_remove: TxHashSet) -> StoreResult<()>;
    fn delete_all(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
}
// Implementations:

#[derive(Clone)]
pub struct DbTxIndexTxEntriesStore {
    access: CachedDbAccess<TransactionId, TxIndexTxEntry>,
    num_of_entries: CachedDbItem<u64>,
}

impl DbTxIndexTxEntriesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { 
            access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::TxIndexTxEntries.into()), 
            num_of_entries: CachedDbItem::new(db, DatabaseStorePrefixes::TxIndexTxEntriesCount.into())
        }
    }
}

impl TxIndexTxEntriesReader for DbTxIndexTxEntriesStore {
    fn get(&self, transaction_id: TransactionId) -> StoreResult<Option<TxOffset>> {
        self.access.read(transaction_id).map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }

    fn has(&self, transaction_id: TransactionId) -> StoreResult<bool> {
        self.access.has(transaction_id)
    }

    fn seek_iterator(&self, from_transaction: Option<TransactionId>, limit: usize, skip_first: bool) ->  TxIndexTxEntriesIterator {
        let seek_key = from_transaction;
        Box::new(self.access.seek_iterator(None, seek_key, limit, skip_first).map(|res| {
            let (key, entry) = res?;
            let transaction_id = TransactionId::from_slice(&key);
            Ok((transaction_id, entry))
        }))
    }

    fn num_of_entries(&self) -> StoreResult<u64> {
        self.num_of_entries.read()
    }

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count(&self) -> StoreResult<usize> {
        Ok(self.access.iterator().count())
    }

}

impl TxIndexTxEntriesStore for DbTxIndexTxEntriesStore {
    fn write_diff_batch(&mut self, batch: &mut WriteBatch, tx_offset_changes: TxOffsetDiff) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        to_remove_count = tx_offset_changes.removed.len();
        to_add_count = tx_offset_changes.added.len();
        self.access.delete_many(&mut writer, &mut tx_offset_changes.removed.iter().cloned())?;
        self.access.write_many(&mut writer, &mut tx_offset_changes.added.iter().map(|(k, v)| (*k, *v)))?;
        self.num_of_entries.update(&mut writer, |num| num + to_add_count as u64 - to_remove_count as u64);
        Ok(())
    }

    fn remove_many(&mut self, batch: &mut WriteBatch, tx_offsets_to_remove: Vec<Hash>) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        let count = tx_offsets_to_remove.len();
        self.access.delete_many(&mut writer, &mut tx_offsets_to_remove.into_iter())?;
        self.num_of_entries.update(&mut writer, |num| num - count);
        Ok(())
    }
    /// Removes all values and keys from the cache and db.
    fn delete_all(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_all(&mut writer);
        self.num_of_entries.write(&mut writer, 0)
    }
}
