use std::sync::{Arc, atomic::Ordering};

use consensus_core::api::DynConsensus;
use rocksdb::DB;
use tokio::{sync::mpsc::{Receiver, channel}, task::JoinHandle};
use super::{
    processes::process_handler::{AtomicUtxoIndexState, UtxoIndexState},
    stores::{circulating_supply::DbCirculatingSupplyStore, utxoindex_tips::DbUtxoIndexTipsStore, utxo_set_by_script_public_key::DbUtxoSetByScriptPublicKeyStore},
};
use consensus::model::stores::virtual_state::VirtualState;

use super::notify::UtxoIndexNotification;
use parking_lot::Mutex;
use tokio::sync::mpsc::{Sender};

pub enum WakeUpSignal{}

#[atomic_enum(AtomicUtxoIndexState)]
#[derive(PartialEq)]
pub enum UtxoIndexState {
    SyncFromDatabase,
    ProcessConsensusEvents,
    SyncFromDatabaseAndProcessConsensusEvents,
    ShutDown,
    Wait,
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
        
        pub state: Arc<AtomicUtxoIndexState>,
        pub signal_chan: Arc<Vec<Sender<WakeUpSignal>, Receiver<WakeUpSignal>>>,
}

impl UtxoIndex {
    pub fn new(consensus: DynConsensus, db: Arc<DB>, recv_chan: Receiver<VirtualState>) -> Self { //TODO: remove db and recv chans once db is complete, and consensus api matures.
        let (s,r) = channel::<WakeUpSignal>(1);
        let signal_chan = Arc::new(vec![s, r]); 
        Self { 
                consensus: consensus,
                consensus_recv: Arc::new(recv_chan),
                
                utxoindex_tips_store: Arc::new(DbUtxoIndexTipsStore::new(db)),
                circulating_suppy_store: Arc::new(DbCirculatingSupplyStore::new(db)),
                utxos_by_script_public_key_store: Arc::new(DbUtxoSetByScriptPublicKeyStore::new(db, 0)),
                
                utxo_diff_by_script_public_key_send: Arc::new(Mutex::new(Vec::new())),
                circulating_supply_send: Arc::new(Mutex::new(Vec::new())),
                tips_send: Arc::new(Mutex::new(Vec::new())),

                state: Arc::new(AtomicUtxoIndexState::new(UtxoIndexState::Wait)),
                signal_chan: Arc::new(vec![s, r]),    
            }
    }

    pub async fn run(&self) {
        loop {
            match self.state.load(Ordering::SeqCst) {
                ProcessConsensusEvents => {
                    while self.state.load(Ordering::SeqCst) == UtxoIndexState::ProcessingConsesnsusEvents{ // event-driven processing state
                        let consensus_event = self.consensus_recv.recv().await.unwrap(); //TODO: handle consensus channel drop.
                        self.process_consensus_event(consensus_event).await;
                    }
                }
                SyncFromDatabase => {
                    self.sync_from_scratch();
                }
                SyncFromDatabaseAndProcessConsensusEvents => {
                    self.sync_from_scratch();
                    self.state.store(UtxoIndexState::ProcessConsensusEvents,  Ordering::SeqCst);
                }
                ShutDown => break, //break out of loop to exit
                Wait => self.signal_chan[1].recv().await //wait for a signal. 
            }
        }
    }

    pub fn signal_process_consensus_events(&self) {
        let former_state = self.state.swap(UtxoIndexState::ProcessConsensusEvents, Ordering::SeqCst);
        if former_state == UtxoIndexState::Wait { self.signal_chan[0].send(WakeUpSignal)}
    }

    pub fn signal_resync_and_process_consensus_events(&self) {
        let former_state = self.state.swap(UtxoIndexState::SyncFromDatabaseAndProcessConsensusEvents, Ordering::SeqCst);
        if former_state == UtxoIndexState::Wait { self.signal_chan[0].send(WakeUpSignal)}
    }

    pub fn signal_resync(&self) {
        let former_state = self.state.swap(UtxoIndexState::SyncFromDatabase,  Ordering::SeqCst);
        if former_state == UtxoIndexState::Wait { self.signal_chan[0].send(WakeUpSignal)}
    }

    pub fn signal_shutdown(&self) {
        let former_state = self.state.swap(UtxoIndexState::ShutDown,  Ordering::SeqCst);
        if former_state == UtxoIndexState::Wait { self.signal_chan[0].send(WakeUpSignal)}
    }


    pub fn signal_wait(&self) {
        self.state.store(UtxoIndexState::Wait,  Ordering::SeqCst);
    }

}
