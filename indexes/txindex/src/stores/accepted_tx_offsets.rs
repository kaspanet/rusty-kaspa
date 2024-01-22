use kaspa_index_core::models::txindex::{TxHashSet, TxOffset, TxOffsetDiff};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use rocksdb::WriteBatch;
use std::sync::Arc;

// Traits:
pub trait TxIndexAcceptedTxOffsetsReader {
    /// Get [`TransactionOffset`] queried by [`TransactionId`],
    fn get(&self, transaction_id: TransactionId) -> StoreResult<Option<TxOffset>>;
    fn has(&self, transaction_id: TransactionId) -> StoreResult<bool>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_keys(&self) -> StoreResult<usize>;
}

pub trait TxIndexAcceptedTxOffsetsStore: TxIndexAcceptedTxOffsetsReader {
    fn write_diff_batch(&mut self, batch: &mut WriteBatch, tx_offset_changes: TxOffsetDiff) -> StoreResult<()>;
    fn remove_many(&mut self, batch: &mut WriteBatch, tx_offsets_to_remove: TxHashSet) -> StoreResult<()>;
    fn delete_all_batched(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
}
// Implementations:

#[derive(Clone)]
pub struct DbTxIndexAcceptedTxOffsetsStore {
    access: CachedDbAccess<TransactionId, TxOffset>,
}

impl DbTxIndexAcceptedTxOffsetsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TxIndexAcceptedOffsets.into()) }
    }
}

impl TxIndexAcceptedTxOffsetsReader for DbTxIndexAcceptedTxOffsetsStore {
    fn get(&self, transaction_id: TransactionId) -> StoreResult<Option<TxOffset>> {
        self.access.read(transaction_id).map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }

    fn has(&self, transaction_id: TransactionId) -> StoreResult<bool> {
        self.access.has(transaction_id)
    }

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_keys(&self) -> StoreResult<usize> {
        Ok(self.access.iterator().count())
    }
}

impl TxIndexAcceptedTxOffsetsStore for DbTxIndexAcceptedTxOffsetsStore {
    fn write_diff_batch(&mut self, batch: &mut WriteBatch, tx_offset_changes: TxOffsetDiff) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut tx_offset_changes.removed.iter().cloned())?;
        self.access.write_many(&mut writer, &mut tx_offset_changes.added.iter().map(|(k, v)| (*k, *v)))?;
        Ok(())
    }

    fn remove_many(&mut self, batch: &mut WriteBatch, tx_offsets_to_remove: TxHashSet) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut tx_offsets_to_remove.iter().cloned())?;
        Ok(())
    }
    /// Removes all [`TxOffsetById`] values and keys from the cache and db.
    fn delete_all_batched(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_all(&mut writer)
    }
}
