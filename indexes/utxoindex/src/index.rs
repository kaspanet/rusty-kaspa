use async_channel::{unbounded, Receiver, Sender};
use std::sync::Arc;
use triggered::{Listener, Trigger};

use consensus::model::stores::{
    errors::{StoreError, StoreResult},
    DB,
};

use consensus_core::{
    api::DynConsensus,
    notify::ConsensusNotification,
    notify::VirtualChangeSetNotification,
    tx::{ScriptPublicKeys, TransactionOutpoint},
    utxo::utxo_diff::UtxoDiff,
    BlockHashSet,
};
use kaspa_core::trace;

use crate::{
    api::UtxoIndexApi,
    errors::UtxoIndexError,
    model::{CirculatingSupply, UtxoSetByScriptPublicKey},
    notify::UtxoIndexNotification,
    stores::store_manager::StoreManager,
    update_container::UtxoIndexChanges,
};
use hashes::Hash;

const RESYNC_CHUNK_SIZE: usize = 2048; //this seems like a sweet spot, and speeds up sync times nearly x2 compared to 1k. (even higher does little).

/// UtxoIndex indexes [`CompactUtxoEntryCollections`] by [`ScriptPublicKey`], commits them to its ownstore, and notifies changes.
#[derive(Clone)]
pub struct UtxoIndex {
    consensus: DynConsensus,

    shutdown_trigger: Trigger,
    pub shutdown_listener: Listener,

    pub stores: StoreManager,
}

impl UtxoIndex {
    /// creates a new [`UtxoIndex`] listening to the passed consensus, and consensus receiver.
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> Self {
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();

        Self { consensus, stores: StoreManager::new(db), shutdown_listener, shutdown_trigger }
    }

    /// Deletes and reinstates the utxoindex database, syncing it from scratch via the consensus database.
    ///
    /// **Note:**
    /// 1) A failure of the call will result in a reset utxoindex database.
    /// 2) There is an implicit expectation that the consensus store most have [VirtualParent] tips. i.e. consensus database most be intiated.
    /// 3) reseting while consensus notifies virtual state changes may result in undefined behaviour.
    pub fn reset(&self) -> Result<(), UtxoIndexError> {
        trace!("resetting the utxoindex");
        self.stores.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;
        let mut from_outpoint = None;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            let virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(from_outpoint, RESYNC_CHUNK_SIZE);

            let mut utxoindex_changes = UtxoIndexChanges::new();

            if virtual_utxo_batch.len() == RESYNC_CHUNK_SIZE {
                //commit batch, remain in the loop.

                let last_outpoint = virtual_utxo_batch.last().expect("expected a none-empty vector").0;
                from_outpoint = Some(TransactionOutpoint::new(last_outpoint.transaction_id, last_outpoint.index + 1)); // Increment index by one, as to not re-retrive with next iteration.

                utxoindex_changes.add_utxo_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxos.added) {
                    Ok(_) => continue, //stay in loop, keep retriving
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            } else {
                //commit remaining utxos and break out of retrival loop

                utxoindex_changes.add_utxo_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxos.added) {
                    Ok(_) => break, //break out of loop, commit other changes
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            }
        }

        //commit to the the remaining stores.

        match self.stores.insert_circulating_supply(circulating_supply) {
            Ok(_) => (),
            Err(err) => {
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        match self.stores.insert_tips(BlockHashSet::from_iter(consensus_tips)) {
            Ok(_) => (),
            Err(err) => {
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        Ok(())
    }

    /// Checks to see if the [UtxoIndex] is sync'd. this is done via comparing the utxoindex commited [VirtualParent] hashes with that of the database.
    fn is_synced(&self) -> Result<bool, UtxoIndexError> {
        // Potential alternative is to use muhash to check sync status.
        trace!("utxoindex checking sync status");
        let utxoindex_tips = self.stores.get_tips();
        match utxoindex_tips {
            Ok(utxoindex_tips) => {
                let consensus_tips = BlockHashSet::from_iter(self.consensus.clone().get_virtual_state_tips());
                let res = *utxoindex_tips == consensus_tips;
                trace!("utxoindex sync status is {res}");
                Ok(res)
            }
            Err(error) => match error {
                StoreError::KeyNotFound(_) => {
                    trace!("utxoindex sync status is {}", false);
                    Ok(false)
                } //Means utxoindex tips database is empty i.e. not sync'd.
                StoreError::KeyAlreadyExists(err) => Err(UtxoIndexError::StoreAccessError(StoreError::KeyAlreadyExists(err))),
                StoreError::DbError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DbError(err))),
                StoreError::DeserializationError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DeserializationError(err))),
            },
        }
    }

    /// Checks if the db is synced, if not, resyncs the database from consensus.
    pub fn maybe_reset(&self) -> Result<(), UtxoIndexError> {
        match self.is_synced() {
            Ok(_) => match self.reset() {
                Ok(_) => {
                    trace!("utxoindex reset");
                    Ok(())
                }
                Err(err) => {
                    self.shutdown_trigger.trigger();
                    Err(err)
                }
            },
            Err(err) => {
                trace!("utxoindex is not synced");
                self.shutdown_trigger.trigger();
                Err(err)
            }
        }
    }

    ///triggers the shutdown, which breaks the async event processing loop, stopping the processing.
    pub fn signal_shutdown(&self) {
        self.shutdown_trigger.trigger();
    }

    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves utxo differences, virtul parent hashes and circulating supply differences to the database.
    /// 2) Notifies all utxo index changes to any potential listeners.
    fn _update(
        &self,
        utxo_set: UtxoDiff,
        tips: Vec<Hash>,
    ) -> Result<Box<dyn Iterator<Item = Arc<UtxoIndexNotification>>>, UtxoIndexError> {
        //return iterator of all utxoindex changes.
        trace!("updating utxoindex with virtual state changes");
        trace!("to remove: {} utxos", utxo_set.remove.len());
        trace!("to add: {} utxos", utxo_set.add.len());

        let mut utxoindex_changes = UtxoIndexChanges::new();

        utxoindex_changes.remove_utxo_collection(utxo_set.remove);
        utxoindex_changes.add_utxo_collection(utxo_set.add);
        utxoindex_changes.add_tips(tips);

        self.stores.update_utxo_state(utxoindex_changes.utxos.clone())?;
        let utxoindex_notifications =
            Box::new(std::iter::once(Arc::new(UtxoIndexNotification::UtxoChanges(utxoindex_changes.utxos.into()))));

        if utxoindex_changes.supply > 0 {
            //force monotonic
            self.stores.update_circulating_supply(utxoindex_changes.supply)?;
        };

        self.stores.insert_tips(utxoindex_changes.tips)?;

        Ok(utxoindex_notifications)
    }

    fn _reset(&self) -> Result<(), UtxoIndexError> {
        trace!("resetting the utxoindex");
        self.stores.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;
        let mut from_outpoint = None;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            let virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(from_outpoint, RESYNC_CHUNK_SIZE);

            let mut utxoindex_changes = UtxoIndexChanges::new();

            if virtual_utxo_batch.len() == RESYNC_CHUNK_SIZE {
                //commit batch, remain in the loop.

                let last_outpoint = virtual_utxo_batch.last().expect("expected a none-empty vector").0;
                from_outpoint = Some(TransactionOutpoint::new(last_outpoint.transaction_id, last_outpoint.index + 1)); // Increment index by one, as to not re-retrive with next iteration.

                utxoindex_changes.add_utxo_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxos.added) {
                    Ok(_) => continue, //stay in loop, keep retriving
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            } else {
                //commit remaining utxos and break out of retrival loop

                utxoindex_changes.add_utxo_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxos.added) {
                    Ok(_) => break, //break out of loop, commit other changes
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            }
        }

        //commit to the the remaining stores.

        match self.stores.insert_circulating_supply(circulating_supply) {
            Ok(_) => (),
            Err(err) => {
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        match self.stores.insert_tips(BlockHashSet::from_iter(consensus_tips)) {
            Ok(_) => (),
            Err(err) => {
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        Ok(())
    }
}

impl UtxoIndexApi for UtxoIndex {
    fn get_circulating_supply(&self) -> StoreResult<u64> {
        self.stores.get_circulating_supply()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        //TODO: chunking
        self.stores.get_utxos_by_script_public_key(script_public_keys)
    }

    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey> {
        self.stores.get_all_utxos()
    }

    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves utxo differences, virtul parent hashes and circulating supply differences to the database.
    /// 2) Notifies all utxo index changes to any potential listeners.
    fn update(
        &self,
        utxo_set: UtxoDiff,
        tips: Vec<Hash>,
    ) -> Result<Box<dyn Iterator<Item = Arc<UtxoIndexNotification>>>, UtxoIndexError> {
        self._update(utxo_set, tips)
    }

    fn reset(&self) -> Result<(), UtxoIndexError> {
        self._reset()
    }
}
