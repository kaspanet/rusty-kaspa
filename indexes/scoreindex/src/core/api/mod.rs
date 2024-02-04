use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_notify::notification::VirtualChainChangedNotification;
use kaspa_consensusmanager::spawn_blocking;

use parking_lot::RwLock;

use crate::{errors::ScoreIndexResult, AcceptingBlueScore, AcceptingBlueScoreHashPair};

///Utxoindex API targeted at retrieval calls.
pub trait ScoreIndexApi: Send + Sync + Debug {
    fn resync(&self) -> ScoreIndexResult<()>;

    fn is_synced(&self) -> ScoreIndexResult<bool>;

    fn get_accepting_blue_score_chain_blocks(
        &self,
        from: AcceptingBlueScore,
        to: AcceptingBlueScore,
    ) -> ScoreIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>>;

    fn get_sink(&self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>>;

    fn get_source(&self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>>;

    fn update_via_virtual_chain_changed(&self, virtual_chain_changed_notification: VirtualChainChangedNotification) -> ScoreIndexResult<()>;
}

/// Async proxy for the UTXO index
#[derive(Debug, Clone)]
pub struct ScoreIndexProxy {
    inner: Arc<RwLock<dyn ScoreIndexApi>>,
}

impl ScoreIndexProxy {
    pub fn new(inner: Arc<RwLock<dyn ScoreIndexApi>>) -> Self {
        Self { inner }
    }

    pub async fn get_accepting_blue_score_chain_blocks(
        self,
        from: AcceptingBlueScore,
        to: AcceptingBlueScore,
    ) -> ScoreIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>> {
        spawn_blocking(move || self.inner.read().get_accepting_blue_score_chain_blocks(from.clone(), to.clone())).await.unwrap()
    }

    pub async fn get_sink(self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>> {
        spawn_blocking(move || self.inner.read().get_sink()).await.unwrap()
    }

    pub async fn get_source(self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>> {
        spawn_blocking(move || self.inner.read().get_source()).await.unwrap()
    }

    pub async fn update_via_virtual_chain_changed(
        self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> ScoreIndexResult<()> {
        spawn_blocking(move || self.inner.write().update_via_virtual_chain_changed(virtual_chain_changed_notification)).await.unwrap()
    }
}
