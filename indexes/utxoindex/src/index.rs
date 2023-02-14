use crate::{
    api::{UtxoIndexControlApi, UtxoIndexRetrievalApi},
    errors::{UtxoIndexError, UtxoIndexResult},
    events::{UtxoIndexEvent, UtxosChangedEvent},
    model::{CirculatingSupply, UtxoSetByScriptPublicKey},
    stores::store_manager::StoreManager,
    update_container::UtxoIndexChanges,
    IDENT,
};

use database::prelude::{StoreError, StoreResult, DB};

use consensus_core::{api::DynConsensus, tx::ScriptPublicKeys, utxo::utxo_diff::UtxoDiff, BlockHashSet};
use hashes::Hash;
use kaspa_core::trace;
use kaspa_utils::arc::ArcExtensions;
use std::sync::Arc;

const RESYNC_CHUNK_SIZE: usize = 2048; //Increased from 1k (used in go-kaspad), for quicker resets, while still having a low memory footprint.

/// UtxoIndex indexes [`CompactUtxoEntryCollections`] by [`ScriptPublicKey`], commits them to its owns tore, and emits changes.
#[derive(Clone)]
pub struct UtxoIndex {
    consensus: DynConsensus,
    stores: StoreManager,
}

impl UtxoIndex {
    /// Creates a new [`UtxoIndex`] listening to the passed consensus, and consensus receiver.
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> Self {
        Self { consensus, stores: StoreManager::new(db) }
    }
}

impl UtxoIndexRetrievalApi for UtxoIndex {
    /// Retrieve circulating supply from the utxoindex db.
    fn get_circulating_supply(&self) -> StoreResult<u64> {
        trace!("[{0}] retrieving circulating supply", IDENT);
        self.stores.get_circulating_supply()
    }

    /// Retrieve utxos by script public keys supply from the utxoindex db.
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        trace!("[{0}] retrieving utxos from {1} script public keys", IDENT, script_public_keys.len());
        self.stores.get_utxos_by_script_public_key(&script_public_keys)
    }

    /// Retrieve the stored tips of the utxoindex (used for testing purposes).
    fn get_utxo_index_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        trace!("[{0}] retrieving tips", IDENT);
        self.stores.get_tips()
    }
}

impl UtxoIndexControlApi for UtxoIndex {
    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves updated utxo differences, virtual parent hashes and circulating supply to the database.
    /// 2) emits an event about utxoindex changes.
    fn update(&self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoIndexEvent> {
        trace!("[{0}] updating...", IDENT);
        trace!("[{0}] adding {1} utxos", IDENT, utxo_diff.add.len());
        trace!("[{0}] removing {1} utxos", IDENT, utxo_diff.remove.len());

        // Initiate update container
        let mut utxoindex_changes = UtxoIndexChanges::new();
        utxoindex_changes.set_utxo_diff(utxo_diff.unwrap_or_clone());
        utxoindex_changes.set_tips(tips.unwrap_or_clone().to_vec());

        // Commit changed utxo state to db
        self.stores.update_utxo_state(&utxoindex_changes.utxo_changes)?;

        // Commit circulating supply change (if monotonic) to db.
        if utxoindex_changes.supply_change > 0 {
            //we force monotonic here
            let _circulating_supply = self.stores.update_circulating_supply(utxoindex_changes.supply_change)?;
        }

        // Commit new consensus virtual tips.
        self.stores.insert_tips(utxoindex_changes.tips)?; //we expect new tips with every virtual!

        // Return the resulting utxoindex event.
        Ok(UtxoIndexEvent::UtxosChanged(Arc::new(UtxosChangedEvent {
            added: Arc::new(utxoindex_changes.utxo_changes.added),
            removed: Arc::new(utxoindex_changes.utxo_changes.removed),
        })))
    }

    /// Checks to see if the [UtxoIndex] is sync'd. This is done via comparing the utxoindex committed `VirtualParent` hashes with those of the consensus database.
    ///
    /// **Note:** Due to sync gaps between the utxoindex and consensus, this function is only reliable while consensus is not processing new blocks.
    fn is_synced(&self) -> UtxoIndexResult<bool> {
        trace!("[{0}] checking sync status...", IDENT);

        let utxoindex_tips = self.stores.get_tips();
        match utxoindex_tips {
            Ok(utxoindex_tips) => {
                let consensus_tips = BlockHashSet::from_iter(self.consensus.clone().get_virtual_state_tips());
                let res = *utxoindex_tips == consensus_tips;
                trace!("[{0}] sync status is {1}", IDENT, res);
                Ok(res)
            }
            Err(error) => match error {
                StoreError::KeyNotFound(_) => {
                    //Means utxoindex tips database is empty i.e. not sync'd.
                    trace!("[{0}] sync status is {1}", IDENT, false);
                    Ok(false)
                }
                other_store_errors => Err(UtxoIndexError::StoreAccessError(other_store_errors)),
            },
        }
    }
    /// Deletes and reinstates the utxoindex database, syncing it from scratch via the consensus database.
    ///
    /// **Notes:**
    /// 1) There is an implicit expectation that the consensus store most have [VirtualParent] tips. i.e. consensus database most be initiated.
    /// 2) resyncing while consensus notifies of utxo differences, may result in a corrupted db.
    fn resync(&self) -> UtxoIndexResult<()> {
        trace!("[{0}] resyncing...", IDENT);
        self.stores.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;

        //Intial batch is without specified seek and none-skipping.
        let mut virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(None, RESYNC_CHUNK_SIZE, false);
        let mut current_chunk_size = virtual_utxo_batch.len();
        while current_chunk_size > 0 {
            // Potential optimization TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply separately.

            let mut utxoindex_changes = UtxoIndexChanges::new(); //reset changes.

            let next_outpoint_from = Some(virtual_utxo_batch.last().expect("expected a last outpoint").0);
            utxoindex_changes.add_utxos_from_vector(virtual_utxo_batch);

            circulating_supply += utxoindex_changes.supply_change as CirculatingSupply;

            trace!("[{0}] resyncing with batch of {1} utxos from consensus db", IDENT, current_chunk_size);

            match self.stores.add_utxo_entries(&utxoindex_changes.utxo_changes.added) {
                Ok(_) => (),
                Err(err) => {
                    trace!("[{0}] resyncing failed, clearing utxoindex db...", IDENT);
                    self.stores.delete_all()?;
                    return Err(UtxoIndexError::StoreAccessError(err));
                }
            };

            if current_chunk_size == RESYNC_CHUNK_SIZE {
                // We expect more utxos.
                virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(next_outpoint_from, RESYNC_CHUNK_SIZE, true);
                current_chunk_size = virtual_utxo_batch.len();
                continue;
            } else {
                //we are finished.
                break;
            }
        }

        // Commit to the the remaining stores.

        trace!("[{0}] committing circulating supply {1} from consensus db", IDENT, circulating_supply);
        match self.stores.insert_circulating_supply(circulating_supply) {
            Ok(_) => (),
            Err(err) => {
                trace!("[{0}] resyncing failed, clearing utxoindex db...", IDENT);
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        trace!("[{0}] committing consensus tips {consensus_tips:?} from consensus db", IDENT);
        match self.stores.insert_tips(BlockHashSet::from_iter(consensus_tips)) {
            Ok(_) => (),
            Err(err) => {
                trace!("[{0}] resyncing failed, clearing utxoindex db...", IDENT);
                self.stores.delete_all()?;
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        Ok(())
    }
}
