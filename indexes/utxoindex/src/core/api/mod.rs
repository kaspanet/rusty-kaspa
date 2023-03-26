use kaspa_consensus_core::{
    tx::{ScriptPublicKeys, TransactionOutpoint},
    utxo::utxo_diff::UtxoDiff,
    BlockHashSet,
};
use kaspa_database::prelude::StoreResult;
use kaspa_hashes::Hash;
use parking_lot::RwLock;
use std::{collections::HashSet, fmt::Debug, sync::Arc};

use crate::{
    errors::UtxoIndexResult,
    model::{UtxoChanges, UtxoSetByScriptPublicKey},
};

///Utxoindex API targeted at retrieval calls.
pub trait UtxoIndexApi: Send + Sync + Debug {
    /// Retrieve circulating supply from the utxoindex db.
    ///
    /// Note: Use a read lock when accessing this method
    fn get_circulating_supply(&self) -> StoreResult<u64>;

    /// Retrieve utxos by script public keys supply from the utxoindex db.
    ///
    /// Note: Use a read lock when accessing this method
    fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;

    // This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>>;

    /// Retrieve the stored tips of the utxoindex (used for testing purposes).
    ///
    /// Note: Use a read lock when accessing this method
    fn get_utxo_index_tips(&self) -> StoreResult<Arc<BlockHashSet>>;

    /// Checks if the utxoindex's db is synced with consensus.
    ///
    /// Note:
    /// 1) Use a read lock when accessing this method
    /// 2) due to potential sync-gaps is_synced is unreliable while consensus is actively resolving virtual states.  
    fn is_synced(&self) -> UtxoIndexResult<bool>;

    /// Update the utxoindex with the given utxo_diff, and tips.
    ///
    /// Note: Use a write lock when accessing this method
    fn update(&mut self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoChanges>;

    /// Resync the utxoindex from the consensus db
    ///
    /// Note: Use a write lock when accessing this method
    fn resync(&mut self) -> UtxoIndexResult<()>;
}

// Below are of the format `Arc<Option<Box<_>>>` because:
// 1) the utxoindex is optional, a `None` in the Option signifies no utxoindex
// 2) there is no need for an inner Arc since we hold an Arc on the Option,
// but alas, we need Sized for the option, hence it is in a Box.

pub type DynUtxoIndexApi = Option<Arc<RwLock<dyn UtxoIndexApi>>>;
