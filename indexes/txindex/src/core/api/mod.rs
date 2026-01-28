use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{BlockAddedNotification, RetentionRootChangedNotification, VirtualChainChangedNotification};
use kaspa_consensusmanager::spawn_blocking;
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use crate::{
    errors::TxIndexResult,
    model::{
        bluescore_refs::{BlueScoreAcceptingRefData, BlueScoreIncludingRefData},
        transactions::{TxAcceptanceData, TxInclusionData},
    },
};

///TxIndex API targeted at retrieval calls.
pub trait TxIndexApi: Send + Sync + Debug {
    fn get_accepted_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>>;
    fn get_included_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>>;
    fn get_transaction_inclusion_data_by_blue_score_range(&self, from: u64, to: u64, limit: Option<usize>) -> TxIndexResult<Vec<BlueScoreIncludingRefData>>;
    fn get_transaction_acceptance_data_by_blue_score_range(&self, from: u64, to: u64, limit: Option<usize>) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>>;

    fn get_sink_with_blue_score(&self) -> TxIndexResult<(Hash, u64)>;

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

    fn prune_on_the_fly(&mut self) -> TxIndexResult<bool>;
    fn is_pruning(&self) -> bool;
    fn toggle_pruning_active(&self, active: bool);
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

    pub async fn async_get_transaction_inclusion_data_by_blue_score_range(self, from: u64, to: u64, limit: Option<usize>) -> TxIndexResult<Vec<BlueScoreIncludingRefData>> {
        spawn_blocking(move || self.inner.read().get_transaction_inclusion_data_by_blue_score_range(from, to, limit)).await.unwrap()
    }

    pub async fn async_get_transaction_acceptance_data_by_blue_score_range(
        self,
        from: u64,
        to: u64,
        limit: Option<usize>,
    ) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>> {
        spawn_blocking(move || self.inner.read().get_transaction_acceptance_data_by_blue_score_range(from, to, limit)).await.unwrap()
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
        spawn_blocking(move || self.inner.write().update_via_retention_root_changed(retention_root_changed_notification)).await.unwrap()
    }

    pub async fn async_prune_on_the_fly(self) -> TxIndexResult<bool> {
        spawn_blocking(move || self.inner.write().prune_on_the_fly()).await.unwrap()
    }

    pub async fn async_is_pruning(self) -> bool {
        spawn_blocking(move || self.inner.read().is_pruning()).await.unwrap()
    }
    pub async fn async_toggle_pruning_active(self, active: bool) {
        spawn_blocking(move || self.inner.write().toggle_pruning_active(active)).await.unwrap()
    }
}
