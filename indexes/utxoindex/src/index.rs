use crate::{
    api::{UtxoIndexControlApi, UtxoIndexRetrivalApi},
    errors::{UtxoIndexError, UtxoIndexResult},
    events::{UtxoIndexEvent, UtxosChangedEvent},
    model::{CirculatingSupply, UtxoSetByScriptPublicKey},
    stores::store_manager::StoreManager,
    update_container::UtxoIndexChanges,
    IDENT,
};

use consensus::model::stores::{
    errors::{StoreError, StoreResult},
    DB,
};
use consensus_core::{
    api::DynConsensus,
    tx::{ScriptPublicKeys, TransactionOutpoint},
    utxo::utxo_diff::UtxoDiff,
    BlockHashSet,
};
use hashes::Hash;
use kaspa_core::trace;
use std::ops::Deref;
use std::sync::Arc;

const RESYNC_CHUNK_SIZE: usize = 2048; //increased from 1k, this gives some quicker resets.

/// UtxoIndex indexes [`CompactUtxoEntryCollections`] by [`ScriptPublicKey`], commits them to its owns tore, and emits changes.
#[derive(Clone)]
pub struct UtxoIndex {
    consensus: DynConsensus,
    pub stores: StoreManager,
}

impl UtxoIndex {
    /// creates a new [`UtxoIndex`] listening to the passed consensus, and consensus receiver.
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> Self {
        Self { consensus, stores: StoreManager::new(db) }
    }
}

impl UtxoIndexRetrivalApi for UtxoIndex {
    fn get_circulating_supply(&self) -> StoreResult<u64> {
        trace!("{0} retriving circulating supply", IDENT);
        self.stores.get_circulating_supply()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        trace!("{0} retriving utxos by from {1} script public keys", IDENT, script_public_keys.len());
        self.stores.get_utxos_by_script_public_key(script_public_keys)
    }

    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey> {
        trace!("{0} retriving utxos", IDENT);
        self.stores.get_all_utxos()
    }
}

impl UtxoIndexControlApi for UtxoIndex {
    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves updated utxo differences, virtul parent hashes and circulating supply to the database.
    /// 2) emits an event about utxoindex changes.
    fn update(&self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoIndexEvent> {
        trace!("{0} updating...", IDENT);
        trace!("{0} adding {1} utxos", IDENT, utxo_diff.add.len());
        trace!("{0} removing {1} utxos", IDENT, utxo_diff.remove.len());
        //intitate update container
        let mut utxoindex_changes = UtxoIndexChanges::new();
        utxoindex_changes.remove_utxo_collection(utxo_diff.deref().remove.to_owned()); // always remove before add! (as the container filters added from removed).
        utxoindex_changes.add_utxo_collection(utxo_diff.deref().add.to_owned());
        utxoindex_changes.add_tips(tips.clone().to_vec());

        //commit changed utxo state
        self.stores.update_utxo_state(utxoindex_changes.utxo_changes.clone())?;

        //commit circulating supply change (if monotonic).
        if utxoindex_changes.supply_change > 0 {
            //we force monotonic here
            let _circulating_supply = self.stores.update_circulating_supply(utxoindex_changes.supply_change)?;
        }

        //commit new consensus virtual tips.
        self.stores.insert_tips(utxoindex_changes.tips)?; //we expect new tips with every virtual!

        //return resulting utxoindex event.
        Ok(UtxoIndexEvent::UtxosChanged(Arc::new(UtxosChangedEvent {
            added: Arc::new(utxoindex_changes.utxo_changes.added),
            removed: Arc::new(utxoindex_changes.utxo_changes.removed),
        })))
    }

    /// Checks to see if the [UtxoIndex] is sync'd. This is done via comparing the utxoindex commited `VirtualParent` hashes with those of the consensus database.
    ///
    /// **Note:** Due to sync gaps between the utxoindex and consensus, this function is only reliable while consensus is not processing new blocks.
    fn is_synced(&self) -> UtxoIndexResult<bool> {
        // Potential alternative is to use muhash to check sync status.
        trace!("{0} checking sync status...", IDENT);
        let utxoindex_tips = self.stores.get_tips();
        match utxoindex_tips {
            Ok(utxoindex_tips) => {
                let consensus_tips = BlockHashSet::from_iter(self.consensus.clone().get_virtual_state_tips());
                let res = *utxoindex_tips == consensus_tips;
                trace!("{0} sync status is {1}", IDENT, res);
                Ok(res)
            }
            Err(error) => match error {
                StoreError::KeyNotFound(_) => {
                    //Means utxoindex tips database is empty i.e. not sync'd.
                    trace!("{0} sync status is {1}", IDENT, false);
                    Ok(false)
                }
                StoreError::KeyAlreadyExists(err) => Err(UtxoIndexError::StoreAccessError(StoreError::KeyAlreadyExists(err))),
                StoreError::DbError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DbError(err))),
                StoreError::DeserializationError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DeserializationError(err))),
            },
        }
    }
    /// Deletes and reinstates the utxoindex database, syncing it from scratch via the consensus database.
    ///
    /// **Notes:**
    /// 1) There is an implicit expectation that the consensus store most have [VirtualParent] tips. i.e. consensus database most be intiated.
    /// 2) reseting while consensus notifies of utxo differences, may result in a corrupted db.
    fn resync(&self) -> UtxoIndexResult<()> {
        trace!("{} resyncing...", IDENT);
        self.stores.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;
        let mut from_outpoint = None;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            let virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(from_outpoint, RESYNC_CHUNK_SIZE);

            trace!("{0} resyncing with batch of {1} utxos from consensus db", IDENT, virtual_utxo_batch.len());

            let mut utxoindex_changes = UtxoIndexChanges::new();

            if virtual_utxo_batch.len() == RESYNC_CHUNK_SIZE {
                //commit batch, remain in the loop.

                let last_outpoint = virtual_utxo_batch.last().expect("expected a none-empty vector").0;
                from_outpoint = Some(TransactionOutpoint::new(last_outpoint.transaction_id, last_outpoint.index + 1)); // Increment index by one, as to not re-retrive with next iteration.

                utxoindex_changes.add_utxo_collection_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply_change as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxo_changes.added) {
                    Ok(_) => continue, //stay in loop, keep retriving
                    Err(err) => {
                        trace!("{0} resyncing failed, clearing utxoindex db...", IDENT);
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            } else {
                //commit remaining utxos to db, and break out of retrival loop

                utxoindex_changes.add_utxo_collection_vector(virtual_utxo_batch);

                circulating_supply += utxoindex_changes.supply_change as CirculatingSupply;

                match self.stores.add_utxo_entries(utxoindex_changes.utxo_changes.added) {
                    Ok(_) => break, //break out of loop, commit other changes
                    Err(err) => {
                        trace!("{0} resyncing failed, clearing utxoindex db...", IDENT);
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                };
            }
        }

        //commit to the the remaining stores.
        trace!("{0} committing circulating supply {1} from consensus db", IDENT, circulating_supply);
        match self.stores.insert_circulating_supply(circulating_supply) {
            Ok(_) => (),
            Err(err) => {
                trace!("{0} resyncing failed, clearing utxoindex db...", IDENT);
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        trace!("{0} committing consensus tips {consensus_tips:?} from consensus db", IDENT);
        match self.stores.insert_tips(BlockHashSet::from_iter(consensus_tips)) {
            Ok(_) => (),
            Err(err) => {
                trace!("{0} resyncing failed, clearing utxoindex db...", IDENT);
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        Ok(())
    }
}
