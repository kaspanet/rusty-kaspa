use async_std::channel::{unbounded, Receiver, Sender};
use futures::{select, FutureExt};
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

const RESYNC_CHUNK_SIZE: usize = 1_000;

/// UtxoIndex indexes [`CompactUtxoEntryCollections`] by [`ScriptPublicKey`], commits them to its ownstore, and notifies changes.
#[derive(Clone)]
pub struct UtxoIndex {
    cons: DynConsensus,
    consensus_recv: Receiver<ConsensusNotification>,
    rpc_sender: Sender<UtxoIndexNotification>,

    pub rpc_receiver: Receiver<UtxoIndexNotification>,

    shutdown_trigger: Trigger,
    pub shutdown_listener: Listener,

    pub shutdown_finalized_trigger: Trigger,
    pub shutdown_finalized_listener: Listener,

    pub stores: StoreManager,
}

impl UtxoIndex {
    /// creates a new [`UtxoIndex`] listening to the passed consensus, and consensus receiver.
    pub fn new(cons: DynConsensus, db: Arc<DB>, consensus_recv: Receiver<ConsensusNotification>) -> Self {
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();
        let (shutdown_finalized_trigger, shutdown_finalized_listener) = triggered::trigger();
        let (rpc_sender, rpc_receiver): (Sender<UtxoIndexNotification>, Receiver<UtxoIndexNotification>) =
            unbounded::<UtxoIndexNotification>();
        Self {
            cons,
            consensus_recv,
            stores: StoreManager::new(db),

            shutdown_listener,
            shutdown_trigger,

            rpc_sender,
            rpc_receiver,

            shutdown_finalized_trigger,
            shutdown_finalized_listener,
        }
    }

    /// Deletes and reinstates the utxoindex database, syncing it from scratch via the consensus database.
    ///
    /// **Note:**
    /// 1) A failure of the call will result in a reset utxoindex database.
    /// 2) There is an implicit expectation that the consensus store most have [VirtualParent] tips. i.e. consensus database most be intiated.
    /// 3) reseting while consensus notifies virtual state changes may result in undefined behaviour.
    fn reset(&self) -> Result<(), UtxoIndexError> {
        trace!("resetting the utxoindex");
        self.stores.delete_all()?;
        let consensus_tips = self.cons.clone().get_virtual_state_tips();
        let mut circulating_supply: CirculatingSupply = 0;
        let mut from_outpoint = None;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            let virtual_utxo_batch = self.cons.clone().get_virtual_utxos(from_outpoint, RESYNC_CHUNK_SIZE);

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

    /// Updates the [UtxoIndex] via the virtual state supplied:
    /// 1) Saves utxo differences, virtul parent hashes and circulating supply differences to the database.
    /// 2) Notifies all utxo index changes to any potential listeners.
    async fn update(&self, virtual_change_set: VirtualChangeSetNotification) -> Result<(), UtxoIndexError> {
        trace!("updating utxoindex with virtual state changes");
        trace!("to remove: {} utxos", virtual_change_set.virtual_utxo_diff.remove.len());
        trace!("to add: {} utxos", virtual_change_set.virtual_utxo_diff.add.len());

        // `impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`.
        let utxoindex_changes: UtxoIndexChanges = virtual_change_set.into(); //`impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`.
        self.stores.update_utxo_state(utxoindex_changes.utxos.clone())?;
        self.rpc_sender.send(UtxoIndexNotification::UtxosChanged(utxoindex_changes.utxos.into())).await?;
        if utxoindex_changes.supply > 0 {
            //force monotonic
            let _circulating_supply = self.stores.update_circulating_supply(utxoindex_changes.supply)?;
        }
        self.stores.insert_tips(utxoindex_changes.tips)?; //we expect new tips with every virtual.
        Ok(())
    }

    /// Checks to see if the [UtxoIndex] is sync'd. this is done via comparing the utxoindex commited [VirtualParent] hashes with that of the database.
    fn is_synced(&self) -> Result<bool, UtxoIndexError> {
        // Potential alternative is to use muhash to check sync status.
        trace!("utxoindex checking sync status");
        let utxoindex_tips = self.stores.get_tips();
        match utxoindex_tips {
            Ok(utxoindex_tips) => {
                let consensus_tips = BlockHashSet::from_iter(self.cons.clone().get_virtual_state_tips());
                let res = *utxoindex_tips == consensus_tips;
                trace!("utxoindex sync status is {}", res);
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

    /// listens to consensus events or a shutdown trigger, and processes those events.
    pub async fn process_events(&self) {
        loop {
            select! {
            _shutdown_signal = self.shutdown_listener.clone().fuse() => break,

            consensus_notification = self.consensus_recv.recv().fuse() => {
                match consensus_notification {
                    Ok(ref msg) => {
                        match msg {
                        ConsensusNotification::VirtualChangeSet(virtual_change_set) => {
                            self.update(virtual_change_set.clone()).await.expect("expected update");
                        },
                        ConsensusNotification::PruningPointUTXOSetOverride(_) => {
                            self.reset().expect("expected reset");
                        }
                        _ => panic!("unexpected consensus notification {:?}", consensus_notification),
                    }
                    }
                    Err(err) => {
                        panic!("{}", UtxoIndexError::ConsensusRecieverUnreachableError(err))
                    }
                    }
                }
            };
        }
        println!("exiting---");
        self.shutdown_finalized_trigger.trigger();
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
}
