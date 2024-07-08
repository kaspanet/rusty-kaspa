use kaspa_consensus::testutils::generate::from_rand::tx;
// External imports
use kaspa_core::{info, trace};
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
        tx_entries::{DbTxIndexTxEntriesStore, TxIndexTxEntriesReader},
        sink_data::DbTxIndexSinkStore, pruning_state::DbTxIndexSourceStore,
        TxIndexSinkStore, TxIndexSourceStore,
    },
    IDENT,
};

use super::{DbTxIndexAcceptedTxEntriesStore, DbTxIndexPruningStateStore};

/// Stores for the transaction index.
pub struct TxIndexStores {
    tx_entries_store: DbTxIndexTxEntriesStore,
    pruning_state_store: DbTxIndexPruningStateStore,
    sink_store: DbTxIndexSinkStore,
    db: Arc<DB>,
}

impl TxIndexStores {
    pub fn new(db: Arc<DB>, config: &Arc<Config>) -> Result<Self, StoreError> {
        // Build cache policies
        let tx_entries_cache_policy = CachePolicyBuilder::new()
            .bytes_budget(config.perf.mem_budget_tx_entries())
            .unit_bytes(config.perf.mem_size_accepted_tx_entries())
            .tracked_bytes()
            .build();

        let tx_entries_store = DbTxIndexTxEntriesStore::new(db.clone(), tx_entries_cache_policy);
        
        // Sanity check
        // TODO: remove when considered stable
        if config.enable_sanity_checks {
            info!("Running sanity checks on txindex stores, This may take a while...");
            assert_eq!(tx_entries_store.num_of_entries()?, tx_entries_store.count()?);
        };

        Ok(Self {
            tx_entries_store,
            pruning_state_store: DbTxIndexPruningStateStore::new(db.clone()),
            sink_store: DbTxIndexSinkStore::new(db.clone()),
            db: db.clone(),
        })
    }

    pub fn tx_entries_store(&self) -> &DbTxIndexTxEntriesStore {
        &self.tx_entries_store
    }

    pub fn pruning_state_store(&self) -> &DbTxIndexPruningStateStore {
        &self.pruning_state_store
    }

    pub fn sink_store(&self) -> &DbTxIndexSinkStore {
        &self.sink_store
    }

    pub fn write_batch(&self, batch: WriteBatch) -> TxIndexResult<()> {
        Ok(self.db.write(batch)?)
    }

    /// Resets the txindex database:
    pub fn delete_all(&mut self, batch: &mut WriteBatch) -> TxIndexResult<()> {
        // TODO: explore possibility of deleting and replacing whole db, currently there is an issue because of file lock and db being in an arc.
        trace!("[{0}] attempting to clear txindex database...", IDENT);

        self.tx_entries_store.delete_all(batch)?;
        self.pruning_state_store.delete_all(batch)?;
        self.sink_store.delete_all(batch)?;

        trace!("[{0}] cleared txindex database", IDENT);

        Ok(())
    }
}

impl Debug for TxIndexStores {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TxIndexStores").finish()
    }
}
