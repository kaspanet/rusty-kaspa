use std::{fs, sync::Arc};

use consensus_core::{
    api::DynConsensus,
    notify::{Notification as ConsensusNotification, VirtualStateChangeNotification},
    tx::{TransactionOutpoint, UtxoEntry},
    BlockHashSet,
};
use futures::channel::oneshot;
use tokio::sync::mpsc::{channel, Receiver};
use triggered::{Listener, Trigger};

use crate::{
    errors::UtxoIndexError,
    model::UtxoIndexChanges,
    notifier::UtxoIndexNotifier,
    stores::StoreManager,
    stores::{
        circulating_supply_store::CirculatingSupplyStore,
        tips_store::{UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
        utxo_set_store::UtxoSetByScriptPublicKeyStore,
    },
};

use super::stores::{
    circulating_supply_store::DbCirculatingSupplyStore, tips_store::DbUtxoIndexTipsStore,
    utxo_set_store::DbUtxoSetByScriptPublicKeyStore,
};
use consensus::model::stores::{errors::StoreError, virtual_state::VirtualState};
use consensus::{
    consensus::Consensus,
    model::stores::{virtual_state, DB},
};

use super::notifier::UtxoIndexNotification;
use parking_lot::Mutex;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::{Receiver as OneShotReceiver, Sender as OneShotSender};

//utxoindex needs to be created after consensus, because it get consensus as a new argument.
//but needs to reset before consensus starts.
pub struct UtxoIndex {
    pub consensus: DynConsensus,

    pub notifier: Arc<Notifier>,

    shutdown_reciever: Arc<OneShotReceiver<()>>,
    pub shutdown_sender: Arc<OneShotSender<()>>,
    shutdown_trigger: Arc<Trigger>,
    pub shutdown_listener: Arc<Listener>,

    stores: Arc<StoreManager>,
}

impl UtxoIndex {
    pub fn new(consensus: Arc<Consensus>, db: Arc<DB>) -> Self {
        let (shutdown_reciever, shutdown_sender) = oneshot::channel::<()>();
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();
        Self {
            consensus: consenus.clone(),

            notifier: Arc::new(Notifier::new()),
            stores: Arc::new(StoreManager::new(db)),

            shutdown_reciever: Arc::new(shutdown_reciever),
            shutdown_sender: Arc::new(shutdown_sender),
            shutdown_listener: Arc::new(shutdown_trigger),
            shutdown_trigger: Arc::new(shutdown_listener),
        }
    }

    pub fn reset(&mut self) -> Result<(), UtxoIndexError> {
        self.store_manager.delete_all();
        let consensus_tips = self.consensus.get_tips();
        let consensus_utxoset_store_iter = self.consensus.get_virtual_utxo_iterator();
        let utxoindex_changes = UtxoIndexChanges::new();
        for store_result in &mut *consensus_utxoset_store_iter.into_iter() {
            //TODO: chunking
            let (transaction_outpoint, utxo_entry) = store_result?;
            utxoindex_changes.add_utxo(transaction_outpoint, utxo_entry)
        }

        match self.store_manager.update_utxo_state(utxoindex_changes.utxo_diff) {
            _ => (),
            Err(err) => {
                self.store_manager.delete_all();
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        match self.store_manager.update_circulating_supply(utxoindex_changes.circulating_supply_diff) {
            _ => (),
            Err(err) => {
                self.store_manager.delete_all();
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };

        match self.store_manager.insert_tips(consensus_tips) {
            _ => (),
            Err(err) => {
                self.store_manager.delete_all();
                return Err(UtxoIndexError::StoreAccessError(err));
            }
        };
        Ok(())
    }

    async fn update(&self, virtual_state: VirtualStateChangeNotification) -> Result<(), UtxoIndexError> {
        trace!("updating utxoindex");
        let utxoindex_changes: UtxoIndexChanges = virtual_state.into(); //`impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`.
        self.store_manager.update_utxo_state(utxoindex_changes.utxo_diff).await?;
        self.notifier.notify_new_utxo_diff_by_script_public_key(utxoindex_changes.utxo_diff).await?; //notifiy utxo changes
        self.store_manager.update_circulating_supply(utxoindex_changes.circulating_supply_diff).await?;
        self.notifier.notify_new_circulating_supply(circulating_supply as u64).await?; //notify circulating supply changes
        self.store_manager.insert_tips(utxoindex_changes.tips).await?;
        self.notifier.notify_new_tips(utxoindex_changes.tips).await?; //notify of new tips
        Ok(())
    }

    fn is_synced(&self) -> Result<bool, UtxoIndexError> {
        trace!("utxoindex checking sync status");
        let store = self.utxoindex_tips_store.lock();
        match store.get() {
            Ok(utxoindex_tips) => {
                let consensus_tips: BlockHashSet = todo!(); //TODO: when querying consensus stores is possible
                Ok(*utxoindex_tips == consensus_tips)
            }
            Err(error) => match error {
                StoreError::KeyNotFound(_) => Ok(false), //simply means utxoindex tips database is empty
                StoreError::KeyAlreadyExists(_) => panic!("key should not already exist"),
                StoreError::DbError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DbError(err))),
                StoreError::DeserializationError(err) => Err(UtxoIndexError::StoreAccessError(StoreError::DeserializationError(err))),
            },
        }
    }

    pub async fn run(&mut self) -> Result<(), UtxoIndexError> {
        //ensure utxoindex is sync'd before running perpetuably
        if !self.is_synced().expect("panic: could not check sync status") {
            trace!("utxoindex is not sync'd");
            self.reset().expect("panic: could not reset db");
        }

        //run and process until signal or error intervens.
        loop {
            tokio::select! {
                consensus_event = self.consensus_recv.recv() => {
                    match self.process_consensus_event(consensus_notification).await {
                        Ok(_) => continue,
                        Err(err) => panic!(err),
                        }
                    }
                shutdown_signal = self.shutdown_reciever.recv() => {
                    match shutdown_signal {
                        Ok(_) => break, //break out of run loop
                        Err(err) => panic!(UtxoIndexError::ShutDownRecieverUnreachableError),
                    }
                }
            }
        }
        trace!("utxoindex is not sync'd");
        self.shutdown_trigger.trigger();
        Ok(())
    }

    async fn process_consensus_event(&self, consensus_notification: ConsensusNotification) -> Result<(), UtxoIndexError> {
        match consensus_notification {
            ConsensusNotification::VirtualStateChange(virtual_state) => self.update(virtual_state).await, //update index with new state
            ConsensusNotification::PruningPointUTXOSetOverride(_) => self.reset(), //new utxo set, so we overwrite
            _ => Ok(()),
        }
    }
}
