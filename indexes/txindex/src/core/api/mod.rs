use crate::core::errors::TxIndexResult;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{
    PruningPointBlueScoreChangedNotification as ConsensusPruningPointBlueScoreChangedNotification,
    VirtualChainChangedNotification as ConsensusVirtualChainChangedNotification,
};
use kaspa_consensusmanager::spawn_blocking;
use kaspa_hashes::Hash;
use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, TxOffset};
use parking_lot::RwLock;
use std::{fmt::Debug, sync::Arc};

pub trait TxIndexApi: Send + Sync + Debug {
    // Sync.

    fn resync(&mut self) -> TxIndexResult<()>;

    fn is_synced(&self) -> TxIndexResult<bool>;

    // Getters:

    fn get_block_acceptance_offset(&self, hash: Hash) -> TxIndexResult<Option<BlockAcceptanceOffset>>;

    fn get_tx_offset(&self, tx_id: TransactionId) -> TxIndexResult<Option<TxOffset>>;

    fn get_sink(&self) -> TxIndexResult<Option<Hash>>;

    fn get_source(&self) -> TxIndexResult<Option<Hash>>;

    // Counters:

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_accepted_tx_offsets(&self) -> TxIndexResult<usize>;
    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_block_acceptance_offsets(&self) -> TxIndexResult<usize>;

    // Updates

    fn update_via_virtual_chain_changed(&mut self, vspcc_notification: ConsensusVirtualChainChangedNotification) -> TxIndexResult<()>;

    fn update_via_pruning_point_blue_score_changed(
        &mut self,
        ppbsc_notification: ConsensusPruningPointBlueScoreChangedNotification,
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

    pub async fn get_block_acceptance_offset(self, hash: Hash) -> TxIndexResult<Option<BlockAcceptanceOffset>> {
        spawn_blocking(move || self.inner.read().get_block_acceptance_offset(hash)).await.unwrap()
    }

    pub async fn update_via_virtual_chain_changed(
        self,
        vspcc_notification: ConsensusVirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(vspcc_notification)).await.unwrap()
    }

    pub async fn update_via_pruning_point_blue_score_changed(
        self,
        ppbsc_notification: ConsensusPruningPointBlueScoreChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_pruning_point_blue_score_changed(ppbsc_notification)
            .await
            .unwrap()
        )
    }
}
