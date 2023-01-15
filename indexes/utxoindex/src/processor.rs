use std::sync::Arc;

use async_trait::async_trait;
use consensus::model::stores::{errors::StoreError, virtual_state::VirtualState};
use consensus_core::{tx::{TransactionOutpoint, UtxoEntry}, BlockHashSet};

use crate::{
    notify::UtxoIndexNotifier, 
    utxoindex::{UtxoIndex, Signal}, 
    model::UtxoIndexChanges, 
    stores::{
        tips_store::{
            UtxoIndexTipsStoreReader, UtxoIndexTipsStore,
        }, 
        circulating_supply_store::CirculatingSupplyStore, utxo_set_store::UtxoSetByScriptPublicKeyStore},
    };

#[async_trait]
pub trait Processor: UtxoIndexNotifier + Send + Sync{
    fn run(&self); //Alias: for `Start` in go-kaspad (since self.start is reserved for service implementation)

    fn reset(&self);

    async fn update(&self, virtual_state: VirtualState);

    fn is_synced(&self) -> bool;

    async fn process_consensus_events(&self); //New: compared to go-kaspad, allows the utxoindex to listen to consensus directly, not via notification manager.  
}

#[async_trait]
impl Processor for UtxoIndex {
    //since Start is reserved for kaspad core service trait, it is renamed here to `run`
    fn run(&self) {
        if !self.is_synced() {
            self.reset();
        }
        tokio::spawn(self.process_consensus_events());
    }

    fn reset(&self) {
        //TODO: delete and reintiate the database. 
        let consensus_tips: BlockHashSet = todo!(); //TODO:  when querying consensus stores is possible
        let consensus_utxoset_store_iter: Arc<dyn Iterator<Item = Result<(TransactionOutpoint, UtxoEntry), StoreError>> + '_> = todo!(); //TODO:  when querying consensus stores is possible
        let utxoindex_changes = UtxoIndexChanges::new();
        for store_result in &mut *consensus_utxoset_store_iter.into_iter() {
            let (transaction_outpoint, utxo_entry) = store_result.expect("expected transaction outpoint / utxo entry pair");
            utxoindex_changes.add_utxo(transaction_outpoint, utxo_entry)
        }
        self.utxos_by_script_public_key_store.write_diff(utxoindex_changes.utxo_diff);
        self.circulating_suppy_store.insert(utxoindex_changes.circulating_supply_diff as u64); //we expect positive result, since we only add. 
        let consensus_tips = todo!();
        self.utxoindex_tips_store.add_tips(consensus_tips);
    }

    async fn update(&self, virtual_state: VirtualState) {
        let utxoindex_changes: UtxoIndexChanges = virtual_state.into(); //`impl From<VirtualState> for UtxoIndexChanges` handles conversion see: `utxoindex::model::utxo_index_changes`. 
        self.utxos_by_script_public_key_store.write_diff(utxoindex_changes.utxo_diff); //update utxo store
        self.notify_new_utxo_diff_by_script_public_key(utxoindex_changes.utxo_diff).await; //notifiy utxo changes
        let circulating_supply = self.circulating_suppy_store.add_circulating_supply_diff(utxoindex_changes.circulating_supply_diff).expect("expected circulating supply");//update circulating supply store
        self.notify_new_circulating_supply(circulating_supply as u64).await; //notify circulating supply changes
        self.utxoindex_tips_store.add_tips(utxoindex_changes.tips); //replace tips in tip store
        self.notify_new_tips(utxoindex_changes.tips).await; //notify of new tips
    }

    fn is_synced(&self) -> bool {
        let utxo_index_tips = self.utxoindex_tips_store.get().expect("");
        let consensus_tips : BlockHashSet = todo!(); //TODO: when querying consensus stores is possible
        *utxo_index_tips == consensus_tips
    }

    async fn process_consensus_events(&self) {
        loop {
            let signal = self.signal_recv.recv();
            let virtual_state =  self.consensus_recv.recv();
            tokio::select!{    
                sig = signal => {
                    match sig {
                        Some(sig) => match sig {
                            Signal::ShutDown => break
                        },
                        None => todo!(), //handle as error
                    }
                }
                virt = virtual_state => {
                    match virt {
                        Some(virt) => self.update(virt),
                        None => todo!(), //handle as error
                    }
                }
            };
        }
    }
}
