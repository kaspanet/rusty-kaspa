use consensus::model::stores::errors::StoreResult;
use consensus_core::tx::ScriptPublicKeys;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use crate::{
    notify::{UtxoIndexNotificationTypes, UtxoIndexNotification, UtxoIndexNotificationType}, 
    stores::{utxoindex_tips::UtxoIndexTipsStoreReader, circulating_supply::CirculatingSupplyStoreReader, utxo_set_by_script_public_key::UtxoSetByScriptPublicKeyStoreReader}};

use super::super::{notify::*, utxoindex::UtxoIndex};

trait UtxoIndexApi: Send + Sync{

    fn get_utxo_indexed_tips(&self) -> StoreResult<Arc<BlockHashSet>>;
    
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>>;

    fn is_synced(&self) -> bool; 

    fn utxoindex_state(&self) -> UtxoIndexState;

    fn register_to_utxoindex_notifictations(&self, utxo_index_notification_types: UtxoIndexNotificationTypes) -> Receiver<UtxoIndexNotification>;

}

impl UtxoIndexApi for UtxoIndex {

    fn get_utxo_indexed_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.utxoindex_tips_store.get()
    }

    fn get_circulating_supply(&self) -> StoreResult<u64>{
        self.circulating_suppy_store.get()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>> { //TODO: chunking
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys(script_public_keys)
    }

    fn is_synced(&self) -> StoreResult<Bool> {
        //TODO: after access to consensus stores / mature consensus api. 
        //compare utxoindexed tips with consensus db tips
        todo!()
    }

    //for debugging, perhaps useful for rpc warn messages. 
    fn query_utxoindex_state(&self) -> UtxoIndexState {
        self.state.load()
    }


    fn register_to_utxoindex_notifictations(&self, utxo_index_notification_types: UtxoIndexNotificationTypes) -> Receiver<UtxoIndexNotification> {
        let (r, s) = channel::<UtxoIndexNotification>(usize::MAX); //TODO: think about what the buffer size should be.
        for utxo_index_notification_type in utxo_index_notification_types.into_iter() {
            match utxo_index_notification_type {
                UtxoIndexNotificationType::UtxoByScriptPublicKeyDiffNotificationType => self.utxo_diff_by_script_public_key_send.lock().push(s),
                UtxoIndexNotificationType::CirculatingSupplyUpdateNotificationType => self.circulating_supply_send.lock().push(s),
                UtxoIndexNotificationType::TipsUpdateNotificationType => self.tips_send.lock().push(s),
                UtxoIndexNotificationType::All => {
                    self.utxo_diff_by_script_public_key_send.lock().push(s);
                    self.circulating_supply_send.lock().push(s);
                    self.tips_send.lock().push(s);
                }
            }
        }
        return r
    }

}

//type DynUtxoIndexApi = Arc::<dyn UtxoIndexApi>;
