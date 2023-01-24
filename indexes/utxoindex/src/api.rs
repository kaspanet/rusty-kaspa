use std::sync::Arc;

use consensus::model::stores::errors::StoreResult;
use consensus_core::tx::ScriptPublicKeys;

use super::model::*;
use super::utxoindex::UtxoIndex;

pub trait UtxoIndexApi: Send + Sync {
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>>;
}

impl UtxoIndexApi for UtxoIndex {
    fn get_circulating_supply(&self) -> StoreResult<u64> {
        self.stores.get_circulating_supply()
    }

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<Arc<UtxoSetByScriptPublicKey>> {
        //TODO: chunking
        self.stores.get_utxos_by_script_public_key(script_public_keys)
    }
}

pub type DynUtxoindex = Arc<dyn UtxoIndexApi>;
