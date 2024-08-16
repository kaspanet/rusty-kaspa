use crate::tasks::{tx::submitter::IndexedTransaction, Stopper, Task};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_consensus_core::tx::Transaction;
use kaspa_core::{info, warn};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::{
    task::JoinHandle,
    time::{sleep, Instant},
};

pub struct TransactionSenderTask {
    client: Arc<GrpcClient>,
    txs: Vec<Arc<Transaction>>,
    tps_pressure: u64,
    mempool_target: u64,
    sender: Sender<IndexedTransaction>,
    stopper: Stopper,
}

impl TransactionSenderTask {
    const UNREGULATED_TPS: u64 = u64::MAX;

    pub fn new(
        client: Arc<GrpcClient>,
        txs: Vec<Arc<Transaction>>,
        tps_pressure: u64,
        mempool_target: u64,
        sender: Sender<IndexedTransaction>,
        stopper: Stopper,
    ) -> Self {
        Self { client, txs, tps_pressure, mempool_target, sender, stopper }
    }

    pub async fn build(
        client: Arc<GrpcClient>,
        txs: Vec<Arc<Transaction>>,
        tps_pressure: u64,
        mempool_target: u64,
        sender: Sender<IndexedTransaction>,
        stopper: Stopper,
    ) -> Arc<Self> {
        Arc::new(Self::new(client, txs, tps_pressure, mempool_target, sender, stopper))
    }

    pub fn sender(&self) -> Sender<IndexedTransaction> {
        self.sender.clone()
    }
}

#[async_trait]
impl Task for TransactionSenderTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let client = self.client.clone();
        let txs = self.txs.clone();
        let regulated_tps_pressure = self.tps_pressure;
        let mempool_target = self.mempool_target;
        let mut tps_pressure = if mempool_target < u64::MAX { Self::UNREGULATED_TPS } else { regulated_tps_pressure };
        // let mut tps_pressure = regulated_tps_pressure;
        let sender = self.sender();
        let stopper = self.stopper;
        let mut last_log_time = Instant::now() - Duration::from_secs(5);
        let mut log_index = 0;
        let task = tokio::spawn(async move {
            warn!("Tx sender task starting...");
            for (i, tx) in txs.into_iter().enumerate() {
                if tps_pressure != Self::UNREGULATED_TPS {
                    sleep(Duration::from_secs_f64(1.0 / tps_pressure as f64)).await;
                }
                if last_log_time.elapsed() > Duration::from_millis(100) {
                    let mut mempool_size = client.get_info().await.unwrap().mempool_size;
                    if log_index % 10 == 0 {
                        info!("Mempool size: {:#?}, txs submitted: {}", mempool_size, i);
                    }
                    log_index += 1;
                    last_log_time = Instant::now();

                    if mempool_size > (mempool_target as f32 * 1.05) as u64 {
                        if tps_pressure != regulated_tps_pressure {
                            warn!("Applying TPS pressure");
                        }
                        tps_pressure = regulated_tps_pressure;
                        while mempool_size > mempool_target {
                            sleep(Duration::from_millis(100)).await;
                            mempool_size = client.get_info().await.unwrap().mempool_size;
                            if log_index % 10 == 0 {
                                info!("Mempool size: {:#?} (targeting {:#?}), txs submitted: {}", mempool_size, mempool_target, i);
                            }
                            log_index += 1;
                        }
                    }
                }
                match sender.send((i, tx)).await {
                    Ok(_) => {}
                    Err(err) => {
                        kaspa_core::error!("Tx sender channel returned error {err}");
                        break;
                    }
                }
                if stop_signal.listener.is_triggered() {
                    break;
                }
            }

            kaspa_core::warn!("Tx sender task, waiting for mempool to drain..");
            let mut prev_mempool_size = u64::MAX;
            loop {
                let mempool_size = client.get_info().await.unwrap().mempool_size;
                info!("Mempool size: {:#?}", mempool_size);
                if mempool_size == 0 || mempool_size == prev_mempool_size {
                    break;
                }
                prev_mempool_size = mempool_size;
                sleep(Duration::from_secs(2)).await;
            }
            if stopper == Stopper::Signal {
                warn!("Tx sender task signaling to stop");
                stop_signal.trigger.trigger();
            }
            sender.close();
            client.disconnect().await.unwrap();
            warn!("Tx sender task exited");
        });
        vec![task]
    }
}
