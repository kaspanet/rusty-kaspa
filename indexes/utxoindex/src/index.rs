use crate::{
    api::UtxoIndexApi,
    errors::{UtxoIndexError, UtxoIndexResult},
    events::{UtxoIndexEvent, UtxosChangedEvent},
    model::{CirculatingSupply, UtxoSetByScriptPublicKey},
    stores::store_manager::Store,
    update_container::UtxoIndexChanges,
    IDENT,
};

use database::prelude::{StoreError, StoreResult, DB};

use consensus_core::{api::DynConsensus, tx::ScriptPublicKeys, utxo::utxo_diff::UtxoDiff, BlockHashSet};
use hashes::Hash;
use kaspa_core::trace;
use kaspa_utils::arc::ArcExtensions;
use parking_lot::RwLock;
use std::sync::Arc;

const RESYNC_CHUNK_SIZE: usize = 2048; //Increased from 1k (used in go-kaspad), for quicker resets, while still having a low memory footprint.

/// UtxoIndex indexes [`CompactUtxoEntryCollections`] by [`ScriptPublicKey`], commits them to its owns store, and emits changes.
/// Note: The UtxoIndex struct by itself is not thread save, only correct usage of the supplied RwLock via `new` makes it so.
/// please follow guidelines found in the comments under `utxoindex::core::api::UtxoIndexApi` for proper thread safety.
pub struct UtxoIndex {
    consensus: DynConsensus,
    store: Store,
}

impl UtxoIndex {
    /// Creates a new [`UtxoIndex`] within a [`RwLock`]
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> UtxoIndexResult<Arc<RwLock<Self>>> {
        let mut utxoindex = Self { consensus, store: Store::new(db) };
        if !utxoindex.is_synced()? {
            utxoindex.resync()?;
        }
        Ok(Arc::new(RwLock::new(utxoindex)))
    }
}
impl UtxoIndexApi for UtxoIndex {
    /// Retrieve circulating supply from the utxoindex db.
    fn get_circulating_supply(&self) -> StoreResult<u64> {
        trace!("[{0}] retrieving circulating supply", IDENT);

        self.store.get_circulating_supply()
    }

    /// Retrieve utxos by script public keys from the utxoindex db.
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        trace!("[{0}] retrieving utxos from {1} script public keys", IDENT, script_public_keys.len());

        self.store.get_utxos_by_script_public_key(&script_public_keys)
    }

    /// Retrieve the stored tips of the utxoindex.
    fn get_utxo_index_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        trace!("[{0}] retrieving tips", IDENT);

        self.store.get_tips()
    }

    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves updated utxo differences, virtual parent hashes and circulating supply to the database.
    /// 2) returns an event about utxoindex changes.
    fn update(&mut self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoIndexEvent> {
        trace!("[{0}] updating...", IDENT);
        trace!("[{0}] adding {1} utxos", IDENT, utxo_diff.add.len());
        trace!("[{0}] removing {1} utxos", IDENT, utxo_diff.remove.len());

        // Initiate update container
        let mut utxoindex_changes = UtxoIndexChanges::new();
        utxoindex_changes.update_utxo_diff(utxo_diff.unwrap_or_clone());
        utxoindex_changes.set_tips(tips.unwrap_or_clone().to_vec());

        // Commit changed utxo state to db
        self.store.update_utxo_state(&utxoindex_changes.utxo_changes.added, &utxoindex_changes.utxo_changes.removed, false)?;

        // Commit circulating supply change (if monotonic) to db.
        if utxoindex_changes.supply_change > 0 {
            //we force monotonic here
            let _circulating_supply =
                self.store.update_circulating_supply(utxoindex_changes.supply_change as CirculatingSupply, false)?;
        }

        // Commit new consensus virtual tips.
        self.store.set_tips(utxoindex_changes.tips, false)?; //we expect new tips with every virtual!

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

        let utxoindex_tips = self.store.get_tips();
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
    /// 1) There is an implicit expectation that the consensus store must have [VirtualParent] tips. i.e. consensus database must be initiated.
    /// 2) resyncing while consensus notifies of utxo differences, may result in a corrupted db.
    fn resync(&mut self) -> UtxoIndexResult<()> {
        trace!("[{0}] resyncing...", IDENT);

        self.store.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;

        //Intial batch is without specified seek and none-skipping.
        let mut virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(None, RESYNC_CHUNK_SIZE, false);
        let mut current_chunk_size = virtual_utxo_batch.len();
        trace!("[{0}] resyncing with batch of {1} utxos from consensus db", IDENT, current_chunk_size);
        // While loop stops resync attemps from an empty utxo db, and unneeded processing when the utxo state size happens to be a multiple of [`RESYNC_CHUNK_SIZE`]
        while current_chunk_size > 0 {
            // Potential optimization TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead (i.e. a potentially uneeded loop),
            // but some form of pre-iteration is done to extract and commit circulating supply separately.

            let mut utxoindex_changes = UtxoIndexChanges::new(); //reset changes.

            let next_outpoint_from = Some(virtual_utxo_batch.last().expect("expected a last outpoint").0);
            utxoindex_changes.add_utxos_from_vector(virtual_utxo_batch);

            circulating_supply += utxoindex_changes.supply_change as CirculatingSupply;

            self.store.update_utxo_state(&utxoindex_changes.utxo_changes.added, &utxoindex_changes.utxo_changes.removed, true)?;

            if current_chunk_size < RESYNC_CHUNK_SIZE {
                break;
            };

            virtual_utxo_batch = self.consensus.clone().get_virtual_utxos(next_outpoint_from, RESYNC_CHUNK_SIZE, true);
            current_chunk_size = virtual_utxo_batch.len();
            trace!("[{0}] resyncing with batch of {1} utxos from consensus db", IDENT, current_chunk_size);
        }

        // Commit to the the remaining stores.

        trace!("[{0}] committing circulating supply {1} from consensus db", IDENT, circulating_supply);
        self.store.insert_circulating_supply(circulating_supply, true)?;

        trace!("[{0}] committing consensus tips {consensus_tips:?} from consensus db", IDENT);
        self.store.set_tips(BlockHashSet::from_iter(consensus_tips), true)?;

        Ok(())
    }

    // This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<std::collections::HashSet<consensus_core::tx::TransactionOutpoint>> {
        self.store.get_all_outpoints()
    }
}
