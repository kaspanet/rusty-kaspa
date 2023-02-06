use std::sync::Arc;

use consensus::model::stores::errors::StoreResult;
use consensus_core::{notify::VirtualChangeSetNotification, tx::ScriptPublicKeys, utxo::utxo_diff::UtxoDiff};

use crate::{errors::UtxoIndexError, model::UtxoSetByScriptPublicKey, notify::UtxoIndexNotification};
use hashes::Hash;

pub trait UtxoIndexApi: Send + Sync {
    /// retrieve circulating supply.
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    /// retrieve utxos by scipt public keys.
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// this is new compared to go-kaspad, and retives all utxos saved in the utxoindex.
    ///
    /// **Warn:**
    ///
    /// this is used only for testing purposes, retriving a full utxo set, in a live setting, is probably never a good idea.
    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// reset the db
    fn reset(&self) -> Result<(), UtxoIndexError>;

    /// update the database via a virtual change set.
    fn update(
        &self,
        utxo_set: UtxoDiff,
        tips: Vec<Hash>,
    ) -> Result<Box<dyn Iterator<Item = Arc<UtxoIndexNotification>>>, UtxoIndexError>;
}

pub type DynUtxoIndex = Arc<dyn UtxoIndexApi>;
