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

    fn get_circulating_supply(&self) -> StoreResult<u64>;

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>>;
}

impl UtxoIndexApi for UtxoIndex {

    fn get_circulating_supply(&self) -> StoreResult<u64> {
        let store = self.circulating_suppy_store.lock();
        store.get()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>> {
        //TODO: chunking
        let store = self.utxos_by_script_public_key_store.lock();
        store.get_utxos_from_script_public_keys(script_public_keys)
    }

}