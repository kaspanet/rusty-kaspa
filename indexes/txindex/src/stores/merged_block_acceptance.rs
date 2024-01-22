use kaspa_consensus_core::{BlockHashSet, BlockHasher};
use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::sync::Arc;

use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, BlockAcceptanceOffsetDiff};

// Traits:

pub trait TxIndexMergedBlockAcceptanceReader {
    /// Get [`TransactionOffset`] queried by [`TransactionId`],
    fn get(&self, block_hash: Hash) -> StoreResult<Option<BlockAcceptanceOffset>>;
    fn has(&self, block_hash: Hash) -> StoreResult<bool>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_keys(&self) -> StoreResult<usize>;
}

pub trait TxIndexMergedBlockAcceptanceStore {
    fn write_diff_batch(
        &mut self,
        batch: &mut WriteBatch,
        block_acceptance_offset_changes: BlockAcceptanceOffsetDiff,
    ) -> StoreResult<()>;
    fn remove_many(&mut self, batch: &mut WriteBatch, block_hashes_to_remove: BlockHashSet) -> StoreResult<()>;
    fn delete_all_batched(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
}

// Implementations:

#[derive(Clone)]
pub struct DbTxIndexMergedBlockAcceptanceStore {
    access: CachedDbAccess<Hash, BlockAcceptanceOffset, BlockHasher>,
}

impl DbTxIndexMergedBlockAcceptanceStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TxIndexMergedBlockAcceptance.into()) }
    }
}

impl TxIndexMergedBlockAcceptanceReader for DbTxIndexMergedBlockAcceptanceStore {
    fn get(&self, block_hash: Hash) -> StoreResult<Option<BlockAcceptanceOffset>> {
        self.access.read(block_hash).map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }

    fn has(&self, block_hash: Hash) -> StoreResult<bool> {
        self.access.has(block_hash)
    }

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_keys(&self) -> StoreResult<usize> {
        Ok(self.access.iterator().count())
    }
}

impl TxIndexMergedBlockAcceptanceStore for DbTxIndexMergedBlockAcceptanceStore {
    fn write_diff_batch(
        &mut self,
        batch: &mut WriteBatch,
        block_acceptance_offset_changes: BlockAcceptanceOffsetDiff,
    ) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut block_acceptance_offset_changes.removed.iter().copied())?;
        self.access.write_many(&mut writer, &mut block_acceptance_offset_changes.added.iter().map(|(k, v)| (*k, *v)))?;
        Ok(())
    }

    fn remove_many(&mut self, batch: &mut WriteBatch, block_hashes_to_remove: BlockHashSet) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut block_hashes_to_remove.iter().cloned())?;
        Ok(())
    }

    /// Removes all [`TxOffsetById`] values and keys from the cache and db.
    fn delete_all_batched(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_all(&mut writer)
    }
}
