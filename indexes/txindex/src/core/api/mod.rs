use crate::core::errors::TxIndexResult;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{
    ChainAcceptanceDataPrunedNotification as ConsensusChainAcceptanceDataPrunedNotification,
    VirtualChainChangedNotification as ConsensusVirtualChainChangedNotification,
};
use kaspa_consensusmanager::spawn_blocking;
use kaspa_hashes::Hash;
use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, TxOffset};
use parking_lot::RwLock;
use std::{fmt::Debug, sync::Arc};

pub trait TxIndexApi: Send + Sync + Debug {
    // Resyncers.

    fn resync(&mut self) -> TxIndexResult<()>;

    // Sync state

    fn is_synced(&self) -> TxIndexResult<bool>;

    // Getters

    fn get_merged_block_acceptance_offset(&self, hash: Hash) -> TxIndexResult<Option<BlockAcceptanceOffset>>;

    fn get_tx_offset(&self, tx_id: TransactionId) -> TxIndexResult<Option<TxOffset>>;

    fn get_sink(&self) -> TxIndexResult<Option<Hash>>;

    fn get_source(&self) -> TxIndexResult<Option<Hash>>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_merged_tx_ids(&self) -> TxIndexResult<usize>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_all_merged_blocks(&self) -> TxIndexResult<usize>;

    // Updates

    fn update_via_virtual_chain_changed(&mut self, vspcc_notification: ConsensusVirtualChainChangedNotification) -> TxIndexResult<()>;

    fn update_via_chain_acceptance_data_pruned(
        &mut self,
        chain_acceptance_data_pruned: ConsensusChainAcceptanceDataPrunedNotification,
    ) -> TxIndexResult<()>;
}

/// Async proxy for the UTXO index
#[derive(Debug, Clone)]
pub struct TxIndexProxy {
    inner: Arc<RwLock<dyn TxIndexApi>>,
}

impl TxIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn TxIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn get_tx_offset(self, tx_id: TransactionId) -> TxIndexResult<Option<TxOffset>> {
        spawn_blocking(move || self.inner.read().get_tx_offset(tx_id)).await.unwrap()
    }

    pub async fn get_merged_block_acceptance_offset(self, hash: Hash) -> TxIndexResult<Option<BlockAcceptanceOffset>> {
        spawn_blocking(move || self.inner.read().get_merged_block_acceptance_offset(hash)).await.unwrap()
    }

    pub async fn update_via_virtual_chain_changed(
        self,
        vspcc_notification: ConsensusVirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(vspcc_notification)).await.unwrap()
    }

    pub async fn update_via_chain_acceptance_data_pruned(
        self,
        chain_acceptance_data_pruned_notification: ConsensusChainAcceptanceDataPrunedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_chain_acceptance_data_pruned(chain_acceptance_data_pruned_notification))
            .await
            .unwrap()
    }
}
