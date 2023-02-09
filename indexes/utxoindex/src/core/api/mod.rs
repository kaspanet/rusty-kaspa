use std::sync::Arc;

use consensus::model::stores::errors::StoreResult;
use consensus_core::{tx::ScriptPublicKeys, utxo::utxo_diff::UtxoDiff};
use hashes::Hash;

use crate::{errors::UtxoIndexResult, events::UtxoIndexEvent, model::UtxoSetByScriptPublicKey};

pub trait UtxoIndexRetrivalApi: Send + Sync {
    // /retrieve circulating supply.
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    /// retrieve utxos by scipt public keys.
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// this is new compared to go-kaspad, and retives all utxos saved in the utxoindex.
    ///
    /// **Warn:**
    ///
    /// this is used only for testing purposes, retriving a full utxo set, in a live setting, is probably never a good idea.
    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey>;
}

pub trait UtxoIndexControlApi: Send + Sync {
    /// updates the utxoindex with the given utxo_diff, and tips.
    fn update(&self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoIndexEvent>;

    /// Resyncs the utxoindex's db from the consensus db
    fn resync(&self) -> UtxoIndexResult<()>;

    /// Checks if the utxoindex's db is synced, if not, resyncs the database from consensus.
    fn is_synced(&self) -> UtxoIndexResult<bool>;
}

pub type DynUtxoIndexRetrivalApi = Arc<Option<Box<dyn UtxoIndexRetrivalApi>>>; //this is an option as utxoindex is not guranteed component!

pub type DynUtxoIndexControlerApi = Arc<Option<Box<dyn UtxoIndexControlApi>>>; //this is an option as utxoindex is not guranteed component!
