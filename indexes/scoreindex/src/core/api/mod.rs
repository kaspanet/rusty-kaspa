use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_notify::notification::{ChainAcceptanceDataPrunedNotification, VirtualChainChangedNotification};
use kaspa_consensusmanager::spawn_blocking;

use parking_lot::RwLock;

use crate::{errors::ConfIndexResult, AcceptingBlueScore, AcceptingBlueScoreHashPair};

///Utxoindex API targeted at retrieval calls.
pub trait ConfIndexApi: Send + Sync + Debug {
    fn resync(&mut self) -> ConfIndexResult<()>;

    fn is_synced(&self) -> ConfIndexResult<bool>;

    fn get_accepting_blue_score_chain_blocks(
        &self,
        from: AcceptingBlueScore,
        to: AcceptingBlueScore,
    ) -> ConfIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>>;

    fn get_sink(&self) -> ConfIndexResult<AcceptingBlueScoreHashPair>;

    fn get_source(&self) -> ConfIndexResult<AcceptingBlueScoreHashPair>;

    fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> ConfIndexResult<()>;

    fn update_via_chain_acceptance_data_pruned(
        &mut self,
        chain_acceptance_data_pruned_notification: ChainAcceptanceDataPrunedNotification,
    ) -> ConfIndexResult<()>;

    //For tests only:

    fn get_all_hash_blue_score_pairs(&self) -> ConfIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>>;
}

/// Async proxy for the UTXO index
#[derive(Debug, Clone)]
pub struct ConfIndexProxy {
    inner: Arc<RwLock<dyn ConfIndexApi>>,
}

impl ConfIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn ConfIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn get_accepting_blue_score_chain_blocks(
        self,
        from: AcceptingBlueScore,
        to: AcceptingBlueScore,
    ) -> ConfIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>> {
        spawn_blocking(move || self.inner.read().get_accepting_blue_score_chain_blocks(from, to)).await.unwrap()
    }

    pub async fn get_sink(self) -> ConfIndexResult<AcceptingBlueScoreHashPair> {
        spawn_blocking(move || self.inner.read().get_sink()).await.unwrap()
    }

    pub async fn get_source(self) -> ConfIndexResult<AcceptingBlueScoreHashPair> {
        spawn_blocking(move || self.inner.read().get_source()).await.unwrap()
    }

    pub async fn update_via_virtual_chain_changed(
        self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> ConfIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(virtual_chain_changed_notification)).await.unwrap()
    }

    pub async fn update_via_chain_acceptance_data_pruned(
        self,
        chain_acceptance_data_pruned_notification: ChainAcceptanceDataPrunedNotification,
    ) -> ConfIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_chain_acceptance_data_pruned(chain_acceptance_data_pruned_notification))
            .await
            .unwrap()
    }
}
