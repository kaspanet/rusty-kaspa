use std::sync::Arc;

use consensus_core::notify::ConsensusNotification;
use consensus_core::{api::DynConsensus, notify::VirtualChangeSetNotification, BlockHashSet};
use kaspa_core::trace;
use triggered::{Listener, Trigger};

use super::{errors::UtxoIndexError, model::UtxoIndexChanges, notify::UtxoIndexNotification, store_manager::StoreManager};
use crate::model::UtxoSetByScriptPublicKey;
use async_std::channel::{unbounded as unbounded_async_std, Receiver as AsyncStdReceiver, Sender as AsyncStdSender};
//use tokio::{sync::mpsc::UnboundedReceiver as TokioUnboundedReceiver, task::JoinError};
use consensus::model::stores::errors::StoreError;
use consensus::model::stores::DB;
use tokio::select;


const RESYNC_CHUNK_SIZE: usize = 1000;

//utxoindex needs to be created after consensus, because it get consensus as a new argument.
//but needs to reset before consensus starts.
#[derive(Clone)]
pub struct UtxoIndex {
    pub consensus: DynConsensus,
    consensus_recv: AsyncStdReceiver<ConsensusNotification>,
    rpc_sender: AsyncStdSender<UtxoIndexNotification>,

    pub rpc_receiver: AsyncStdReceiver<UtxoIndexNotification>,

    pub shutdown_trigger: Trigger,
    pub shutdown_listener: Listener,
    pub shutdown_finalized_trigger: Trigger,
    pub shutdown_finalized_listener: Listener,

    pub stores: StoreManager,
}

impl UtxoIndex {

    ///creates a new utxoindex
    pub fn new(consensus: DynConsensus, db: Arc<DB>, consensus_recv: AsyncStdReceiver<ConsensusNotification>) -> Self {
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();
        let (shutdown_finalized_trigger, shutdown_finalized_listener) = triggered::trigger();
        let (rpc_sender, rpc_receiver): (AsyncStdSender<UtxoIndexNotification>, AsyncStdReceiver<UtxoIndexNotification>) =
            unbounded_async_std::<UtxoIndexNotification>();
        Self {
            consensus,
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
    pub fn reset(&self) -> Result<(), UtxoIndexError> {
        trace!("resetting the utxoindex");
        self.stores.delete_all()?;
        let consensus_tips = self.consensus.clone().get_virtual_state_tips();
        let mut utxoindex_changes = UtxoIndexChanges::new();
        let start_outpoint = None;
        let circulating_supply: i64 = 0;
        loop {
            // potential TODO: iterating virtual utxos into an [UtxoIndexChanges] struct is a bit of overhead,
            // but some form of pre-iteration is needed to extract and commit circulating supply seperatly.
            // alternative is to merge all individual stores, or handle this logic within the utxoindex store_manager.
            let mut batch_processed: usize = 0;
            for (transaction_outpoint, utxo_entry) in
                self.consensus.clone().get_virtual_utxos(start_outpoint, RESYNC_CHUNK_SIZE).iter()
            {
                utxoindex_changes.add_utxo(transaction_outpoint, utxo_entry);
                batch_processed += 1;
                if batch_processed == RESYNC_CHUNK_SIZE {
                    let start_outpoint = Some(transaction_outpoint);
                }
            }
            if utxoindex_changes.utxos.added.len() < RESYNC_CHUNK_SIZE {
                match self.stores.insert_utxo_entries(utxoindex_changes.utxos.added) {
                    _ => (),
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                }
                match self.stores.insert_circulating_supply(circulating_supply as u64) {
                    _ => (),
                    Err(err) => {
                        self.stores.delete_all()?;
                        return Err(UtxoIndexError::StoreAccessError(err));
                    }
                }
                break;
            };
            match self.stores.insert_utxo_entries(utxoindex_changes.utxos.added) {
                _ => (),
                Err(err) => {
                    self.stores.delete_all()?;
                    return Err(UtxoIndexError::StoreAccessError(err));
                }
            }
            utxoindex_changes.utxos.added = UtxoSetByScriptPublicKey::new();
        }

        match self.stores.insert_tips(BlockHashSet::from_iter(consensus_tips)) {
            _ => (),
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
            //force monotonic circulating supply here.
            let _circulating_supply = self.stores.update_circulating_supply(utxoindex_changes.supply)?;
            //TODO: circulating supply update notifications in rpc -> uncomment line below when done.
            //self.rpc_sender.send(UtxoIndexNotification::CirculatingSupplyNotification(CirculatingSupplyNotification::new(circulating_supply))).await;
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
                let consensus_tips = BlockHashSet::from_iter(self.consensus.clone().get_virtual_state_tips()); //TODO: when querying consensus stores is possible
                let res = *utxoindex_tips == consensus_tips;
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

    ///checks if the db is synced, if not resyncs the the database
    pub fn maybe_reset(&self) -> Result<(), UtxoIndexError> {
        match self.is_synced() {
            Ok(_) => match self.reset() {
                Ok(_) => Ok(()),
                Err(err) => {
                    self.shutdown_trigger.trigger();
                    Err(err)
                }
            },
            Err(err) => {
                trace!("utxoindex is not synced");
                self.shutdown_trigger.trigger();
                return Err(err);
            }
        }
    }

    ///triggers the shutdown which breaks the async event processing loop
    pub fn signal_shutdown(&self) {
        self.shutdown_trigger.trigger();
    }

    ///resyncs the utxoindex database, if not synced and processes events.
    pub fn run(&self) {
        self.maybe_reset().expect("expected maybe_reset not to err");
        self.process_events();
    }

    /// syncs the database, if unsynced, and listens to consensus events or a shut-down signal and handles / processes those events.
    pub fn process_events(&self) {
        let mut _self = self.clone();
        tokio::spawn( async move { loop {
            select! {
                _shutdown_signal = async { _self.shutdown_listener.wait() } => break,

                consensus_notification = _self.consensus_recv.recv() => {
                    match consensus_notification {
                        Ok(ref msg) => match msg {
                            ConsensusNotification::VirtualChangeSet(virtual_change_set) => _self.update(virtual_change_set.clone()).await.expect("expected update"),
                            ConsensusNotification::PruningPointUTXOSetOverride(_) => _self.reset().expect("expected reset"),
                            _ => panic!("unexpected consensus notification {:?}", consensus_notification),
                        }
                        Err(err) => panic!("{}", err),
                        }
                    }
                };
            }
            _self.shutdown_finalized_trigger.trigger();
        });
    }
}
