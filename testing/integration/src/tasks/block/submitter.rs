use crate::{
    common::daemon::ClientManager,
    tasks::{Stopper, Task},
};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_core::warn;
use kaspa_grpc_client::ClientPool;
use kaspa_rpc_core::{api::rpc::RpcApi, RpcRawBlock};
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::sleep};

pub struct BlockSubmitterTask {
    pool: ClientPool<RpcRawBlock>,
    stopper: Stopper,
}

impl BlockSubmitterTask {
    pub fn new(pool: ClientPool<RpcRawBlock>, stopper: Stopper) -> Self {
        Self { pool, stopper }
    }

    pub async fn build(client_manager: Arc<ClientManager>, pool_size: usize, stopper: Stopper) -> Arc<Self> {
        let pool = client_manager.new_client_pool(pool_size, 100).await;
        Arc::new(Self::new(pool, stopper))
    }

    pub fn sender(&self) -> Sender<RpcRawBlock> {
        self.pool.sender()
    }
}

#[async_trait]
impl Task for BlockSubmitterTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        warn!("Block submitter task starting...");
        let mut tasks = self.pool.start(|c, block: RpcRawBlock| async move {
            loop {
                match c.submit_block(block.clone(), false).await {
                    Ok(response) => {
                        assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
                        break;
                    }
                    Err(_) => {
                        sleep(Duration::from_millis(50)).await;
                    }
                }
            }
            false
        });

        let pool_shutdown_listener = self.pool.shutdown_listener();
        let sender = self.sender();
        let stopper = self.stopper;
        let shutdown_task = tokio::spawn(async move {
            tokio::select! {
                _ = stop_signal.listener.clone() => {}
                _ = pool_shutdown_listener.clone() => {
                    if stopper == Stopper::Signal {
                        stop_signal.trigger.trigger();
                    }
                }
            }
            let _ = sender.close();
            pool_shutdown_listener.await;
            warn!("Block submitter task exited");
        });
        tasks.push(shutdown_task);

        tasks
    }
}
