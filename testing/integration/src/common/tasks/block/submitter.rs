use crate::common::{daemon::ClientManager, tasks::Task};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_grpc_client::ClientPool;
use kaspa_rpc_core::{api::rpc::RpcApi, RpcBlock};
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct BlockSubmitterTask {
    pool: ClientPool<RpcBlock>,
}

impl BlockSubmitterTask {
    pub fn new(pool: ClientPool<RpcBlock>) -> Self {
        Self { pool }
    }

    pub async fn build(client_manager: Arc<ClientManager>, pool_size: usize) -> Arc<Self> {
        let pool = client_manager
            .new_client_pool(pool_size, 100, |c, block: RpcBlock| async move {
                let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("sb");
                loop {
                    match c.submit_block(block.clone(), false).await {
                        Ok(response) => {
                            assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
                            break;
                        }
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    }
                }
                false
            })
            .await;
        Arc::new(Self::new(pool))
    }

    pub fn sender(&self) -> Sender<RpcBlock> {
        self.pool.sender()
    }
}

#[async_trait]
impl Task for BlockSubmitterTask {
    fn start(&self, _stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        self.pool.join_handles()
    }
}
