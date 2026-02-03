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

/// TxIndex Test API
pub trait TxIndexTestAPI: TxIndexApi {
    fn get_all_transaction_acceptance_refs(&self) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>>;

    fn get_all_transaction_inclusion_refs(&self) -> TxIndexResult<Vec<DaaScoreIncludingRefData>>;

    fn get_tips(&self) -> TxIndexResult<Option<Arc<BlockHashSet>>>;
    fn get_retention_root(&self) -> TxIndexResult<Option<Hash>>;
}

///TxIndex API
pub trait TxIndexApi: Send + Sync + Debug {
    fn get_accepted_transaction_data(&self, transaction_id: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>>;
    fn get_included_transaction_data(&self, transaction_id: TransactionId) -> TxIndexResult<Vec<TxInclusionData>>;
    fn get_sink_with_blue_score(&self) -> TxIndexResult<(Hash, u64)>;

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

impl TxIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn TxIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn async_get_accepted_transaction_data(self, transaction_id: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        spawn_blocking(move || self.inner.read().get_accepted_transaction_data(transaction_id)).await.unwrap()
    }

    pub async fn async_get_included_transaction_data(self, transaction_id: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        spawn_blocking(move || self.inner.read().get_included_transaction_data(transaction_id)).await.unwrap()
    }

    pub async fn async_get_sink_with_blue_score(self) -> TxIndexResult<(Hash, u64)> {
        spawn_blocking(move || self.inner.read().get_sink_with_blue_score()).await.unwrap()
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

/// Async proxy for the TxIndex
#[derive(Debug, Clone)]
pub struct TxIndexProxy {
    inner: Arc<RwLock<dyn TxIndexApi>>,
}

impl Deref for TxIndexProxy {
    type Target = RwLock<dyn TxIndexApi>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
