use std::sync::Arc;

use consensus::model::stores::errors::StoreResult;
use consensus_core::tx::ScriptPublicKeys;

use super::model::UtxoSetByScriptPublicKey;

pub trait UtxoIndexApi: Send + Sync {
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// this is new compared to go-kaspad, and retives all utxos saved in the utxoindex.
    ///
    /// **Warn:**
    ///
    /// this is used only for testing purposes, retriving a full utxo set, in a live setting, is probably never a good idea.
    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey>;
}

pub type DynUtxoIndex = Arc<dyn UtxoIndexApi>;
