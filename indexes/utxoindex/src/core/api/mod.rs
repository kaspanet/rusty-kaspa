use kaspa_consensus_core::{
    tx::{ScriptPublicKeys, TransactionOutpoint},
    utxo::utxo_diff::UtxoDiff,
    BlockHashSet,
};
use kaspa_consensusmanager::spawn_blocking;
use kaspa_database::prelude::StoreResult;
use kaspa_hashes::Hash;
use kaspa_index_core::indexed_utxos::BalanceByScriptPublicKey;
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

    fn get_balance_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey>;

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

/// Async proxy for the UTXO index
#[derive(Debug, Clone)]
pub struct UtxoIndexProxy {
    inner: Arc<RwLock<dyn UtxoIndexApi>>,
}

impl UtxoIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn UtxoIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn get_circulating_supply(self) -> StoreResult<u64> {
        spawn_blocking(move || self.inner.read().get_circulating_supply()).await.unwrap()
    }

    pub async fn get_utxos_by_script_public_keys(self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        spawn_blocking(move || self.inner.read().get_utxos_by_script_public_keys(script_public_keys)).await.unwrap()
    }

    pub async fn get_balance_by_script_public_keys(
        self,
        script_public_keys: ScriptPublicKeys,
    ) -> StoreResult<BalanceByScriptPublicKey> {
        spawn_blocking(move || self.inner.read().get_balance_by_script_public_keys(script_public_keys)).await.unwrap()
    }

    pub async fn update(self, utxo_diff: Arc<UtxoDiff>, tips: Arc<Vec<Hash>>) -> UtxoIndexResult<UtxoChanges> {
        spawn_blocking(move || self.inner.write().update(utxo_diff, tips)).await.unwrap()
    }
}
