use std::{fs, sync::Arc};

use consensus_core::{
    api::DynConsensus,
    notify::VirtualChangeSetNotification,
    tx::{TransactionOutpoint, UtxoEntry},
    BlockHashSet,
};
use kaspa_core::trace;
use triggered::{Listener, Trigger};

use super::{errors::UtxoIndexError, model::UtxoIndexChanges, notify::UtxoIndexNotification, store_manager::StoreManager};

use consensus::model::stores::errors::StoreError;
use consensus::{
    consensus::Consensus,
    model::stores::{virtual_state, DB},
};

use super::notifier::UtxoIndexNotification;

const RESYNC_CHUNK_SIZE: usize = 1000;

//utxoindex needs to be created after consensus, because it get consensus as a new argument.
//but needs to reset before consensus starts.
pub struct UtxoIndex {
    pub consensus: DynConsensus,

    pub shutdown_trigger: Arc<Trigger>,
    pub shutdown_listener: Arc<Listener>,

    stores: Arc<StoreManager>,
}

impl UtxoIndex {
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> Self {
        let (Dyncshutdown_trigger, shutdown_listener) = triggered::trigger();
        Self {
            consensus,
            stores: Arc::new(StoreManager::new(db)),
            shutdown_listener: Arc::new(shutdown_trigger),
            shutdown_trigger: Arc::new(shutdown_listener),
        }
    }

    /// Deletes and reinstates the utxoindex database, syncing it from scratch via the consensus database.
    ///
    /// **Note:**
    /// 1) A failure of the call will result in a reset utxoindex database.
    /// 2) There is an implcit expectation that the consensus store most have [VirtualParent] tips. i.e. consensus database most be intiated.
    pub fn reset(&mut self) -> Result<(), UtxoIndexError> {
        trace!("resetting the utxoindex");
        self.stores.delete_all();
        let consensus_tips = self.consensus.get_virtual_state_tips();
        let utxoindex_changes = UtxoIndexChanges::new();
        start_outpoint = todo!();
        utxoindex_changes = UtxoIndexChanges::new();
        let circulating_supply = i64;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            for (transaction_outpoint, utxo_entry) in self.consensus.get_virtual_utxos(start_outpoint, RESYNC_CHUNK_SIZE) {
                utxoindex_changes.add_utxo(transaction_output, utxo_entry);
                self.stores.insert_utxo_entries(utxoindex_changes.utxo_diff.added)?;
                if virtual_utxo_chunk.len() < RESYNC_CHUNK_SIZE {
                    self.stores.insert_circulating_supply(circulating_supply as u64)?;
                    drop(utxoindex_changes);
                    break;
                };
                utxoindex_changes.clear();
            }
        }

        match self.stores.insert_tips(consensus_tips)? {
            _ => (),
            Err(err) => {
                self.stores.delete_all();
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };
        Ok(())
    }

    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves utxo differences, virtul parent hashes and circulating supply differences to the database.
    /// 2) Notifies all utxo index changes to any potential listeners.
    async fn update(&self, virtual_change_set: VirtualChangeSetNotification) -> Result<(Vec<UtxoIndexNotification>), UtxoIndexError> {
        trace!("updating utxoindex with virtual state changes");
        trace!("to remove: {} utxos", virtual_change_set.virtual_utxo_diff.remove.len());
        trace!("to add: {} utxos", virtual_change_set.virtual_utxo_diff.add.len());

        // `impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`.
        let utxoindex_changes: UtxoIndexChanges = virtual_change_set.into(); //`impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`.
        let notifications = Vec::new::<UtxoIndexNotification>();
        self.stores.update_utxo_state(utxoindex_changes.utxos);
        notifications.push(UtxoIndexNotification::UtxosChanged(utxoindex_changes.utxos.into()));
        if utxoindex_changes.supply > 0 { //force monotonic circulating supply here.
            circulating_supply = self.stores.update_circulating_supply(utxoindex_changes.circulating_supply_diff).await?;
            //TODO: circulating supply update notifications in rpc -> uncomment line below when done.
            //notifications.push(UtxoIndexNotification::CirculatingSupplyNotification(CirculatingSupplyNotification::new(circulating_supply)));
        }
        self.stores.insert_tips(utxoindex_changes.tips).await?; //we expect new tips with every virtual.
        Ok((notifications))
    }

    /// Checks to see if the [UtxoIndex] is sync'd. this is done via comparing the utxoindex commited [VirtualParent] hashes with that of the database.
    fn is_synced(&self) -> Result<bool, UtxoIndexError> {
        // Potential alternative is to use muhash to check sync status.
        trace!("utxoindex checking sync status");
        let utxoindex_tips = self.stores.get_tips();
        match utxoindex_tips {
            Ok(utxoindex_tips) => {
                let consensus_tips = BlockHashSet::from(self.consensus.get_virtual_state_tips()); //TODO: when querying consensus stores is possible
                let res = utxoindex_tips == consensus_tips;
                trace!("sync status is {}", res);
                Ok(res)
            }
            Err(error) => match error {
                StoreError::KeyNotFound(_) => {
                    trace!("utxoindex status is {}", false);
                    Ok(false)
                } //simply means utxoindex tips database is empty //TODO: handle this case, since we try to sync without it actually being possible.
                StoreError::KeyAlreadyExists(err) => Err(UtxoIndexError::StoreAccessError(StoreError::KeyAlreadyExists(err))),
                StoreError::DbError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DbError(err))),
                StoreError::DeserializationError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DeserializationError(err))),
            },
        }
    }

    /// syncs the database, if unsynced, and listens to consensus events or a shut-down signal and handles / processes those events.
    pub async fn run(&mut self) -> Result<(), UtxoIndexError> {
        // ensure utxoindex is sync'd before running perpetually
        match self.is_synced() {
            Ok(_) => match self.reset() {
                Ok(_) => Ok(()),
                Err(err) => {
                    self.shutdown_trigger.trigger();
                    err
                }
            },
            Err(err) => {
                trace!("utxoindex is not synced");
                self.shutdown_trigger.trigger();
                return err;
            }
        }
    }
}
