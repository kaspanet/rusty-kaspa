// External imports
use kaspa_core::trace;
use kaspa_database::{
    cache_policy_builder::CachePolicyBuilder,
    prelude::{StoreError, DB},
};
use std::{
    fmt::{self, Debug, Formatter},
    sync::Arc,
};

use rocksdb::WriteBatch;

// Local imports
use crate::{
    config::Config,
    errors::TxIndexResult,
    stores::{
        accepted_tx_offsets::DbTxIndexAcceptedTxOffsetsStore, merged_block_acceptance::DbTxIndexMergedBlockAcceptanceStore,
        sink::DbTxIndexSinkStore, source::DbTxIndexSourceStore, TxIndexAcceptedTxOffsetsStore, TxIndexMergedBlockAcceptanceStore,
        TxIndexSinkStore, TxIndexSourceStore,
    },
};

/// Stores for the transaction index.
pub struct TxIndexStores {
    pub accepted_tx_offsets_store: DbTxIndexAcceptedTxOffsetsStore,
    pub merged_block_acceptance_store: DbTxIndexMergedBlockAcceptanceStore,
    pub source_store: DbTxIndexSourceStore,
    pub sink_store: DbTxIndexSinkStore,
    db: Arc<DB>,
}

impl TxIndexStores {
    pub fn new(db: Arc<DB>, config: &Arc<Config>) -> Result<Self, StoreError> {
        // Build cache policies
        let tx_offset_cache_policy = CachePolicyBuilder::new()
            .bytes_budget(config.perf.mem_budget_tx_offset())
            .unit_bytes(config.perf.mem_size_tx_offset())
            .tracked_bytes()
            .build();

        let block_acceptance_cache_policy = CachePolicyBuilder::new()
            .bytes_budget(config.perf.mem_budget_block_acceptance_offset())
            .unit_bytes(config.perf.mem_size_block_acceptance_offset())
            .tracked_bytes()
            .build();

        Ok(Self {
            accepted_tx_offsets_store: DbTxIndexAcceptedTxOffsetsStore::new(db.clone(), tx_offset_cache_policy),
            merged_block_acceptance_store: DbTxIndexMergedBlockAcceptanceStore::new(db.clone(), block_acceptance_cache_policy),
            source_store: DbTxIndexSourceStore::new(db.clone()),
            sink_store: DbTxIndexSinkStore::new(db.clone()),
            db: db.clone(),
        })
    }

    pub fn write_batch(&self, batch: WriteBatch) -> TxIndexResult<()> {
        Ok(self.db.write(batch)?)
    }

    /// Resets the txindex database:
    pub fn delete_all(&mut self) -> TxIndexResult<()> {
        // TODO: explore possibility of deleting and replacing whole db, currently there is an issue because of file lock and db being in an arc.
        trace!("[{0:?}] attempting to clear txindex database...", self);

        let mut batch: rocksdb::WriteBatchWithTransaction<false> = WriteBatch::default();

        self.source_store.remove_batch_via_batch_writer(&mut batch)?;
        self.sink_store.remove_batch_via_batch_writer(&mut batch)?;
        self.accepted_tx_offsets_store.delete_all_batched(&mut batch)?;
        self.merged_block_acceptance_store.delete_all_batched(&mut batch)?;

        self.db.write(batch)?;

        trace!("[{0:?}] cleared txindex database", self);

        Ok(())
    }
}

impl Debug for TxIndexStores {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TxIndexStores").finish()
    }
}
