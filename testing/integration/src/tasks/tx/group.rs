use crate::{
    common::daemon::ClientManager,
    tasks::{
        tx::{sender::TransactionSenderTask, submitter::TransactionSubmitterTask},
        Stopper, Task,
    },
};
use async_trait::async_trait;
use itertools::chain;
use kaspa_consensus_core::tx::Transaction;
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct TxSenderGroupTask {
    submitter: Arc<TransactionSubmitterTask>,
    sender: Arc<TransactionSenderTask>,
}

impl TxSenderGroupTask {
    pub fn new(submitter: Arc<TransactionSubmitterTask>, sender: Arc<TransactionSenderTask>) -> Self {
        Self { submitter, sender }
    }

    pub async fn build(
        client_manager: Arc<ClientManager>,
        submitter_pool_size: usize,
        allow_orphan: bool,
        txs: Vec<Arc<Transaction>>,
        tps_pressure: u64,
        mempool_target: u64,
        stopper: Stopper,
    ) -> Arc<Self> {
        // Tx submitter
        let submitter = TransactionSubmitterTask::build(client_manager.clone(), submitter_pool_size, allow_orphan, stopper).await;

        // Tx sender
        let client = Arc::new(client_manager.new_client().await);
        let sender = TransactionSenderTask::build(client, txs, tps_pressure, mempool_target, submitter.sender(), stopper).await;

        Arc::new(Self::new(submitter, sender))
    }
}

#[async_trait]
impl Task for TxSenderGroupTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        chain![self.submitter.start(stop_signal.clone()), self.sender.start(stop_signal.clone())].collect()
    }
}
