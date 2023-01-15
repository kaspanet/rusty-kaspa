use std::sync::Arc;

use consensus_core::api::DynConsensus;
use tokio::sync::mpsc::{Receiver, channel};

use super::{
    stores::{circulating_supply_store::DbCirculatingSupplyStore, tips_store::DbUtxoIndexTipsStore, utxo_set_store::DbUtxoSetByScriptPublicKeyStore},
};
use consensus::model::stores::{virtual_state::VirtualState};
use consensus::model::stores::DB;

use super::notify::UtxoIndexNotification;
use parking_lot::Mutex;
use tokio::sync::mpsc::{Sender};

#[derive(PartialEq)]
pub enum Signal{
    ShutDown,
}
//utxoindex needs to be created after consensus, because it get consensus as a new argument.
//but needs to reset before consensus starts. 
pub struct UtxoIndex {
        pub consensus: DynConsensus,
        pub consensus_recv: Arc<Receiver<VirtualState>>,

        pub utxoindex_tips_store: DbUtxoIndexTipsStore,
        pub circulating_suppy_store: DbCirculatingSupplyStore, 
        pub utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore, 
        
        pub utxo_diff_by_script_public_key_send: Arc<Mutex<Vec<Sender<UtxoIndexNotification>>>>,
        pub circulating_supply_send: Arc<Mutex<Vec<Sender<UtxoIndexNotification>>>>,
        pub tips_send:  Arc<Mutex<Vec<Sender<UtxoIndexNotification>>>>,
        
        pub signal_send: Arc<Sender<Signal>>, 
        pub signal_recv: Arc<Receiver<Signal>>,
}

impl UtxoIndex {
    pub fn new(consensus: DynConsensus, db: Arc<DB>) -> Self { //TODO: remove db and recv chans once db is complete, and consensus api matures.
        let (s,r) = channel::<Signal>(1);
        Self { 
                consensus: consensus,
                consensus_recv: todo!(), //TODO: once consenus notifications are active
                
                utxoindex_tips_store: DbUtxoIndexTipsStore::new(db),
                circulating_suppy_store: DbCirculatingSupplyStore::new(db),
                utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore::new(db, 0),
                
                utxo_diff_by_script_public_key_send: Arc::new(Mutex::new(Vec::new())),
                circulating_supply_send: Arc::new(Mutex::new(Vec::new())),
                tips_send: Arc::new(Mutex::new(Vec::new())),

                signal_recv: Arc::new(r),
                signal_send: Arc::new(s),
            }
    }

}
