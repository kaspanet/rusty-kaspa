use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_core::BlockHashSet;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{
    BlockAddedNotification, RetentionRootChangedNotification, VirtualChainChangedNotification,
};
use kaspa_consensusmanager::spawn_blocking;
use kaspa_hashes::Hash;
use parking_lot::RwLock;
use std::ops::Deref;
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    errors::TxIndexResult,
    model::{
        score_refs::{BlueScoreAcceptingRefData, DaaScoreIncludingRefData},
        {TxAcceptanceData, TxInclusionData},
    },
};

///TxIndex API targeted at retrieval calls.
pub trait TxIndexApi: Send + Sync + Debug {
    fn get_accepted_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>>;
    fn get_included_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>>;
    fn get_transaction_inclusion_data_by_daa_score_range(
        &self,
        from: u64,
        to: u64,
        limit: Option<usize>,
        limit_to_score_boundary: bool,
    ) -> TxIndexResult<Vec<DaaScoreIncludingRefData>>;
    fn get_transaction_acceptance_data_by_blue_score_range(
        &self,
        from: u64,
        to: u64,
        limit: Option<usize>,
        limit_to_score_boundary: bool,
    ) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>>;

    fn get_sink_with_blue_score(&self) -> TxIndexResult<(Hash, u64)>;
    fn get_tips(&self) -> TxIndexResult<Option<Arc<BlockHashSet>>>;
    fn get_retention_root(&self) -> TxIndexResult<Option<Hash>>;

    fn is_synced(&self) -> TxIndexResult<bool>;
    fn is_acceptance_data_synced(&self) -> TxIndexResult<bool>;
    fn is_inclusion_data_synced(&self) -> TxIndexResult<bool>;
    fn is_retention_synced(&self) -> TxIndexResult<bool>;

    fn resync_all_from_scratch(&mut self) -> TxIndexResult<()>;
    fn resync_acceptance_data_from_scratch(&mut self) -> TxIndexResult<()>;
    fn resync_inclusion_data_from_scratch(&mut self) -> TxIndexResult<()>;
    fn resync_retention_data_from_scratch(&mut self) -> TxIndexResult<()>;

    fn update_via_block_added(&mut self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()>;
    fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()>;
    fn update_via_retention_root_changed(
        &mut self,
        retention_root_changed_notification: RetentionRootChangedNotification,
    ) -> TxIndexResult<()>;

    fn prune_batch(&mut self) -> TxIndexResult<bool>;
    fn get_pruning_lock(&self) -> Arc<AsyncMutex<()>>;
}

/// Async proxy for the TxIndex
#[derive(Debug, Clone)]
pub struct TxIndexProxy {
    inner: Arc<RwLock<dyn TxIndexApi>>,
}

impl TxIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn TxIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn async_get_accepted_transaction_data(self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        spawn_blocking(move || self.inner.read().get_accepted_transaction_data(txid)).await.unwrap()
    }

    pub async fn async_get_included_transaction_data(self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        spawn_blocking(move || self.inner.read().get_included_transaction_data(txid)).await.unwrap()
    }

    pub async fn async_get_transaction_inclusion_data_by_blue_score_range(
        self,
        from: u64,
        to: u64,
        limit: Option<usize>,
        limit_to_score_boundary: bool,
    ) -> TxIndexResult<Vec<DaaScoreIncludingRefData>> {
        spawn_blocking(move || {
            self.inner.read().get_transaction_inclusion_data_by_daa_score_range(from, to, limit, limit_to_score_boundary)
        })
        .await
        .unwrap()
    }

    pub async fn async_get_transaction_acceptance_data_by_blue_score_range(
        self,
        from: u64,
        to: u64,
        limit: Option<usize>,
        limit_to_score_boundary: bool,
    ) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>> {
        spawn_blocking(move || {
            self.inner.read().get_transaction_acceptance_data_by_blue_score_range(from, to, limit, limit_to_score_boundary)
        })
        .await
        .unwrap()
    }

    pub async fn async_get_sink_with_blue_score(self) -> TxIndexResult<(Hash, u64)> {
        spawn_blocking(move || self.inner.read().get_sink_with_blue_score()).await.unwrap()
    }

    pub async fn async_get_tips(self) -> TxIndexResult<Option<Arc<BlockHashSet>>> {
        spawn_blocking(move || self.inner.read().get_tips()).await.unwrap()
    }

    pub async fn get_retention_root(self) -> TxIndexResult<Option<Hash>> {
        spawn_blocking(move || self.inner.read().get_retention_root()).await.unwrap()
    }

    pub async fn async_update_via_block_added(self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_block_added(block_added_notification)).await.unwrap()
    }

    pub async fn async_update_via_virtual_chain_changed(
        self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(virtual_chain_changed_notification)).await.unwrap()
    }

    pub async fn async_update_via_retention_root_changed(
        self,
        retention_root_changed_notification: RetentionRootChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_retention_root_changed(retention_root_changed_notification))
            .await
            .unwrap()
    }

    pub async fn async_prune_batch(self) -> TxIndexResult<bool> {
        spawn_blocking(move || self.inner.write().prune_batch()).await.unwrap()
    }

    pub async fn async_get_pruning_lock(&self) -> Arc<AsyncMutex<()>> {
        let inner = self.inner.clone();
        spawn_blocking(move || inner.read().get_pruning_lock()).await.unwrap()
    }
}

impl Deref for TxIndexProxy {
    type Target = RwLock<dyn TxIndexApi>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
