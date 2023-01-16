use std::sync::Arc;

use crate::{
    notifier::{UtxoIndexNotification, UtxoIndexNotificationType, UtxoIndexNotificationTypes},
    stores::{
        circulating_supply_store::CirculatingSupplyStoreReader, tips_store::UtxoIndexTipsStoreReader,
        utxo_set_store::UtxoSetByScriptPublicKeyStoreReader,
    },
};
use consensus::model::stores::errors::StoreResult;
use consensus_core::{tx::ScriptPublicKeys, BlockHashSet};
use tokio::sync::mpsc::{channel, Receiver};

use super::model::*;
use super::utxoindex::UtxoIndex;

trait UtxoIndexApi: Send + Sync {
    fn get_utxo_indexed_tips(&self) -> StoreResult<Arc<BlockHashSet>>;

    fn get_circulating_supply(&self) -> StoreResult<u64>;

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>>;

    fn register_to_utxoindex_notifictations(
        &self,
        utxo_index_notification_types: UtxoIndexNotificationTypes,
    ) -> Receiver<UtxoIndexNotification>;
}

impl UtxoIndexApi for UtxoIndex {
    fn get_utxo_indexed_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        let store = self.utxoindex_tips_store.lock();
        store.get()
    }

    fn get_circulating_supply(&self) -> StoreResult<u64> {
        let store = self.circulating_suppy_store.lock();
        store.get()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>> {
        //TODO: chunking
        let store = self.utxos_by_script_public_key_store.lock();
        store.get_utxos_from_script_public_keys(script_public_keys)
    }

    fn register_to_utxoindex_notifictations(
        &self,
        utxo_index_notification_types: UtxoIndexNotificationTypes,
    ) -> Receiver<UtxoIndexNotification> {
        let (s, r) = channel::<UtxoIndexNotification>(usize::MAX); //TODO: think about what the buffer size should be.
        for utxo_index_notification_type in utxo_index_notification_types.into_iter() {
            match utxo_index_notification_type {
                UtxoIndexNotificationType::UtxoByScriptPublicKeyDiffNotificationType => {
                    self.utxo_diff_by_script_public_key_send.lock().push(s.clone())
                }
                UtxoIndexNotificationType::CirculatingSupplyUpdateNotificationType => {
                    self.circulating_supply_send.lock().push(s.clone())
                }
                UtxoIndexNotificationType::TipsUpdateNotificationType => self.tips_send.lock().push(s.clone()),
                UtxoIndexNotificationType::All => {
                    self.utxo_diff_by_script_public_key_send.lock().push(s.clone());
                    self.circulating_supply_send.lock().push(s.clone());
                    self.tips_send.lock().push(s.clone());
                }
            }
        }
        return r;
    }
}

//type DynUtxoIndexApi = Arc::<dyn UtxoIndexApi>;
