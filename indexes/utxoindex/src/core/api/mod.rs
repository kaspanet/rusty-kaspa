use std::sync::Arc;

use consensus_core::{tx::ScriptPublicKeys, utxo::utxo_diff::UtxoDiff, BlockHashSet};
use database::prelude::StoreResult;
use hashes::Hash;

use crate::{errors::UtxoIndexResult, events::UtxoIndexEvent, model::UtxoSetByScriptPublicKey};

///Utxoindex API targeted at retrieval calls.
pub trait UtxoIndexRetrievalApi: Send + Sync {
    /// Retrieve circulating supply from the utxoindex db.
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    /// Retrieve utxos by script public keys supply from the utxoindex db.
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// This is new compared to go-kaspad, and retrieves all utxos saved in the utxoindex.
    ///
    /// **Warn:**
    ///
    /// this is used only for testing purposes, retrieving a full utxo set, in a live setting,it is probably never a good idea.
    fn get_all_utxos(&self) -> StoreResult<UtxoSetByScriptPublicKey>;

    /// Retrieve the stored tips of the utxoindex (used for testing purposes).
    fn get_utxo_index_tips(&self) -> StoreResult<Arc<BlockHashSet>>;
}

///Utxoindex API targeted at Controlling the utxoindex.
pub trait UtxoIndexControlApi: Send + Sync {
    /// Update the utxoindex with the given utxo_diff, and tips.
    fn update(&self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoIndexEvent>;

    /// Resync the utxoindex from the consensus db
    fn resync(&self) -> UtxoIndexResult<()>;

    /// Checks if the utxoindex's db is synced, if not, resync the database from consensus.
    fn is_synced(&self) -> UtxoIndexResult<bool>;
}

// Below are of the format `Arc<Option<Box<_>>>` because:
// 1) the utxoindex is optional, a `None` in the Option signifies no utxoindex
// 2) there is no need for an inner Arc since we hold an Arc on the Option,
// but alas, we need Sized for the option, hence it is in a Box.

pub type DynUtxoIndexRetrievalApi = Arc<Option<Box<dyn UtxoIndexRetrievalApi>>>;

pub type DynUtxoIndexControllerApi = Arc<Option<Box<dyn UtxoIndexControlApi>>>;
