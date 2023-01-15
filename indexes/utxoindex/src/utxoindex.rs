use std::sync::{Arc, atomic::Ordering};

use consensus_core::{api::DynConsensus;
use tokio::{sync::mpsc::{Receiver, channel}};
use crate::{notify::UtxoIndexNotifier, stores::{tips_store::UtxoIndexTipsStoreReader, utxo_set_store::UtxoSetByScriptPublicKeyStore}, errors::UtxoIndexError};

use super::{
    stores::{circulating_supply_store::DbCirculatingSupplyStore, tips_store::DbUtxoIndexTipsStore, utxo_set_store::DbUtxoSetByScriptPublicKeyStore},
};
use consensus::model::stores::virtual_state::VirtualState;
use consensus::model::stores::DB;

use super::notify::UtxoIndexNotification;
use super::model::*;
use parking_lot::Mutex;
use tokio::sync::mpsc::{Sender};

pub enum Signal{
    ShutDown,
}
//utxoindex needs to be created after consensus, because it get consensus as a new argument.
//but needs to reset before consensus starts. 
pub struct UtxoIndex {
        pub consensus: DynConsensus,
        pub consensus_recv: Arc<Receiver<VirtualState>>,

        pub utxoindex_tips_store: Arc<DbUtxoIndexTipsStore>,
        pub circulating_suppy_store: Arc<DbCirculatingSupplyStore>, 
        pub utxos_by_script_public_key_store: Arc<DbUtxoSetByScriptPublicKeyStore>, 
        
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
                
                utxoindex_tips_store: Arc::new(DbUtxoIndexTipsStore::new(db)),
                circulating_suppy_store: Arc::new(DbCirculatingSupplyStore::new(db)),
                utxos_by_script_public_key_store: Arc::new(DbUtxoSetByScriptPublicKeyStore::new(db, 0)),
                
                utxo_diff_by_script_public_key_send: Arc::new(Mutex::new(Vec::new())),
                circulating_supply_send: Arc::new(Mutex::new(Vec::new())),
                tips_send: Arc::new(Mutex::new(Vec::new())),

                signal_recv: Arc::new(r),
                signal_send: Arc::new(s),
            }
    }

    //since Start is reserved for kaspad core service trait, it is renamed here to `run`
    pub fn run(&self) -> Result<(), E> {
        if !self.is_synced()? {
            self.reset();
        }
        tokio::spawn(self.process_consensus_events())
    }

    pub fn reset(&self) {
        //TODO: delete and reintiate the database. 
        while !self.consensus_recv.is_empty() { self.consensus_recv.recv() } //drain and discard the channel
        let consensus_utxoset_store_iter= todo!(); //TODO:  when querying consensus stores is possible
        let utxoindex_changes = UtxoIndexChanges::new();
        for (transation_outpoint, utxo_entry) in consensus_utxoset_store_iter.iter() {
            utxoindex_changes.add_utxo(transation_outpoint, utxo_entry)
        }
        self.utxos_by_script_public_key_store.write_diff(&utxoindex_changes.utxo_diff);
        self.circulating_suppy_store.update(utxoindex_changes.circulating_supply_diff);
        let consensus_tips = todo!();
        self.utxoindex_tips_store.update(consensus_tips);

    }

    pub async fn update(&self, virtual_state: VirtualState){
        let utxoindex_changes: UtxoIndexChanges = virtual_state.into(); //`impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`. 
        self.utxos_by_script_public_key_store.write_diff(&utxoindex_changes.utxo_diff).await; //update utxo store
        self.notify_new_utxo_diff_by_script_public_key(utxoindex_changes.utxo_diff).await; //notifiy utxo changes
        let circulating_supply = self.circulating_suppy_store.update(utxoindex_changes.circulating_supply_diff).await;//update circulating supply store
        self.notify_new_circulating_supply(circulating_supply as u64).await; //notify circulating supply changes
        self.utxoindex_tips_store.update(utxoindex_changes.tips).await; //replace tips in tip store
        self.notify_new_tips(utxoindex_changes.tips).await; //notify of new tips
    }

    pub fn is_synced(&self) -> bool {
        let utxo_index_tips = self.utxoindex_tips_store.get().expect("");
        let consensus_tips = todo!(); //TODO: when querying consensus stores is possible
        Ok(utxo_index_tips == consensus_tips)
    }

    async fn process_consensus_events(&mut self) {
        loop {
            tokio::select!{
                //biased; //We want signals to have greater priority.
    
                signal = self.signal_recv.recv() => {
                   match signal {
                        Some(signal) => todo!(),
                        None => match self.signal_recv.try_recv()  { //we do this to extract the error
                        
                        },
                };
                virtual_state = self.consensus_recv.recv() => {
                    match virtual_state {
                        Some(virtual_state) => self.update()
                        None => match self.consensus_recv.try_recv() {

                        }
                    }
                }
            }
        }
    }
}

}
