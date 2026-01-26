use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{BlockAddedNotification, VirtualChainChangedNotification};
use kaspa_consensusmanager::spawn_blocking;
use parking_lot::RwLock;

use crate::{
    errors::TxIndexResult,
    model::{
        bluescore_refs::BlueScoreRefData,
        transactions::{TxAcceptanceData, TxInclusionData},
    },
};

///TxIndex API targeted at retrieval calls.
pub trait TxIndexApi: Send + Sync + Debug {
    fn get_accepted_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>>;
    fn get_included_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>>;
    fn get_transaction_data_by_blue_score_range(&self, from: u64, to: u64) -> TxIndexResult<Vec<BlueScoreRefData>>;
    fn get_transaction_inclusion_data_by_blue_score_range(&self, from: u64, to: u64) -> TxIndexResult<Vec<BlueScoreRefData>>;
    fn get_transaction_acceptance_data_by_blue_score_range(&self, from: u64, to: u64) -> TxIndexResult<Vec<BlueScoreRefData>>;

    fn update_via_block_added(&mut self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()>;
    fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()>;
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

    pub async fn get_accepted_transaction_data(self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        spawn_blocking(move || self.inner.read().get_accepted_transaction_data(txid)).await.unwrap()
    }

    pub async fn get_included_transaction_data(self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        spawn_blocking(move || self.inner.read().get_included_transaction_data(txid)).await.unwrap()
    }

    pub async fn get_transaction_data_by_blue_score_range(self, from: u64, to: u64) -> TxIndexResult<Vec<BlueScoreRefData>> {
        spawn_blocking(move || self.inner.read().get_transaction_data_by_blue_score_range(from, to)).await.unwrap()
    }

    pub async fn get_transaction_inclusion_data_by_blue_score_range(self, from: u64, to: u64) -> TxIndexResult<Vec<BlueScoreRefData>> {
        spawn_blocking(move || self.inner.read().get_transaction_inclusion_data_by_blue_score_range(from, to)).await.unwrap()
    }

    pub async fn get_transaction_acceptance_data_by_blue_score_range(
        self,
        from: u64,
        to: u64,
    ) -> TxIndexResult<Vec<BlueScoreRefData>> {
        spawn_blocking(move || self.inner.read().get_transaction_acceptance_data_by_blue_score_range(from, to)).await.unwrap()
    }

    pub async fn update_via_block_added(self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_block_added(block_added_notification)).await.unwrap()
    }

    pub async fn update_via_virtual_chain_changed(
        self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(virtual_chain_changed_notification)).await.unwrap()
    }
}
