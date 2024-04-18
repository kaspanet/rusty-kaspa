use crate::{
    common::daemon::ClientManager,
    tasks::{Stopper, Task},
};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_consensus_core::tx::Transaction;
use kaspa_core::{error, warn};
use kaspa_grpc_client::ClientPool;
use kaspa_rpc_core::{api::rpc::RpcApi, RpcError};
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::sleep};

pub type IndexedTransaction = (usize, Arc<Transaction>);

/// Transaction submitter
///
/// Pay close attention to the submission dynamic and its effect on orphans. The safe configuration is
/// a single worker and disallowing orphans. If multiple workers are configured, `allow_orphan` should
/// be set to true unless some special test use case (like fully independent txs) is requiring to
/// disallow orphans so the test can fail.
pub struct TransactionSubmitterTask {
    pool: ClientPool<IndexedTransaction>,
    allow_orphan: bool,
    stopper: Stopper,
}

impl TransactionSubmitterTask {
    const MAX_ATTEMPTS: usize = 5;

    pub fn new(pool: ClientPool<IndexedTransaction>, allow_orphan: bool, stopper: Stopper) -> Self {
        Self { pool, allow_orphan, stopper }
    }

    pub async fn build(client_manager: Arc<ClientManager>, pool_size: usize, allow_orphan: bool, stopper: Stopper) -> Arc<Self> {
        let pool = client_manager.new_client_pool(pool_size, 100).await;
        Arc::new(Self::new(pool, allow_orphan, stopper))
    }

    pub fn sender(&self) -> Sender<IndexedTransaction> {
        self.pool.sender()
    }
}

#[async_trait]
impl Task for TransactionSubmitterTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        warn!("Transaction submitter task starting...");
        let mut tasks = match self.allow_orphan {
            false => self.pool.start(|c, (i, tx)| async move {
                for attempt in 0..Self::MAX_ATTEMPTS {
                    match c.submit_transaction(tx.as_ref().into(), false).await {
                        Ok(_) => {
                            return false;
                        }
                        Err(RpcError::General(msg)) if msg.contains("orphan") => {
                            error!("Transaction {i}: submit attempt #{attempt} failed");
                            error!("\n\n\n{msg}\n\n");
                            sleep(Duration::from_millis(50)).await;
                        }
                        Err(e) => panic!("{e}"),
                    }
                }
                panic!("Failed to submit transaction {i} after {} attempts", Self::MAX_ATTEMPTS);
            }),
            true => self.pool.start(|c, (_, tx)| async move {
                match c.submit_transaction(tx.as_ref().into(), true).await {
                    Ok(_) => {}
                    Err(e) => panic!("{e}"),
                }
                false
            }),
        };

        let pool_shutdown_listener = self.pool.shutdown_listener();
        let sender = self.sender();
        let stopper = self.stopper;
        let shutdown_task = tokio::spawn(async move {
            tokio::select! {
                _ = stop_signal.listener.clone() => {}
                _ = pool_shutdown_listener.clone() => {
                    if stopper == Stopper::Signal {
                        warn!("Transaction submitter task signaling to stop");
                        stop_signal.trigger.trigger();
                    }
                }
            }
            let _ = sender.close();
            pool_shutdown_listener.await;
            warn!("Transaction submitter task exited");
        });
        tasks.push(shutdown_task);

        tasks
    }
}
