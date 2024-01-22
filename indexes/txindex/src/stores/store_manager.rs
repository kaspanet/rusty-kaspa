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
        accepted_tx_offsets::DbTxIndexAcceptedTxOffsetsStore, block_acceptance_offsets::DbTxIndexBlockAcceptanceOffsetsStore,
        history_root::DbTxIndexHistoryRootStore, sink::DbTxIndexSinkStore, TxIndexAcceptedTxOffsetsStore,
        TxIndexBlockAcceptanceOffsetsStore, TxIndexHistoryRootStore, TxIndexSinkStore,
    },
    IDENT,
};

/// Stores for the transaction index.
pub struct TxIndexStores {
    pub accepted_tx_offsets_store: DbTxIndexAcceptedTxOffsetsStore,
    pub block_acceptance_offsets_store: DbTxIndexBlockAcceptanceOffsetsStore,
    pub history_root_store: DbTxIndexHistoryRootStore,
    pub sink_store: DbTxIndexSinkStore,
    db: Arc<DB>,
}

impl TxIndexStores {
    pub fn new(db: Arc<DB>, config: &Arc<Config>) -> Result<Self, StoreError> {
        // Build cache policies
        let accepted_tx_offsets_cache_policy = CachePolicyBuilder::new()
            .bytes_budget(config.perf.mem_budget_tx_offset())
            .unit_bytes(config.perf.mem_size_tx_offset())
            .tracked_bytes()
            .build();

        let block_acceptance_offsets_cache_policy = CachePolicyBuilder::new()
            .bytes_budget(config.perf.mem_budget_block_acceptance_offset())
            .unit_bytes(config.perf.mem_size_block_acceptance_offset())
            .tracked_bytes()
            .build();

        Ok(Self {
            accepted_tx_offsets_store: DbTxIndexAcceptedTxOffsetsStore::new(db.clone(), accepted_tx_offsets_cache_policy),
            block_acceptance_offsets_store: DbTxIndexBlockAcceptanceOffsetsStore::new(
                db.clone(),
                block_acceptance_offsets_cache_policy,
            ),
            history_root_store: DbTxIndexHistoryRootStore::new(db.clone()),
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
        trace!("[{0}] attempting to clear txindex database...", IDENT);

        let mut batch: rocksdb::WriteBatchWithTransaction<false> = WriteBatch::default();

        self.history_root_store.remove(&mut batch)?;
        self.sink_store.remove(&mut batch)?;
        self.accepted_tx_offsets_store.delete_all(&mut batch)?;
        self.block_acceptance_offsets_store.delete_all(&mut batch)?;

        self.db.write(batch)?;

        trace!("[{0}] cleared txindex database", IDENT);

        Ok(())
    }
}

impl Debug for TxIndexStores {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TxIndexStores").finish()
    }
}
