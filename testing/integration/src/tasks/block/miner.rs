use crate::tasks::{Stopper, Task};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_addresses::Address;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, GetBlockTemplateResponse, RpcRawBlock};
use kaspa_utils::triggers::SingleTrigger;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{
    cmp::max,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{task::JoinHandle, time::sleep};

pub const COMMUNICATION_DELAY: u64 = 1_000;

pub struct BlockMinerTask {
    client: Arc<GrpcClient>,
    bps: u64,
    block_count: usize,
    sender: Sender<RpcRawBlock>,
    template: Arc<Mutex<GetBlockTemplateResponse>>,
    pay_address: Address,
    tx_counter: Arc<AtomicUsize>,
    comm_delay: u64,
    stopper: Stopper,
}

impl BlockMinerTask {
    pub fn new(
        client: Arc<GrpcClient>,
        bps: u64,
        block_count: usize,
        sender: Sender<RpcRawBlock>,
        template: Arc<Mutex<GetBlockTemplateResponse>>,
        pay_address: Address,
        stopper: Stopper,
    ) -> Self {
        Self {
            client,
            bps,
            block_count,
            sender,
            template,
            pay_address,
            tx_counter: Default::default(),
            comm_delay: COMMUNICATION_DELAY,
            stopper,
        }
    }

    pub async fn build(
        client: Arc<GrpcClient>,
        bps: u64,
        block_count: usize,
        sender: Sender<RpcRawBlock>,
        template: Arc<Mutex<GetBlockTemplateResponse>>,
        pay_address: Address,
        stopper: Stopper,
    ) -> Arc<Self> {
        Arc::new(Self::new(client, bps, block_count, sender, template, pay_address, stopper))
    }

    pub fn sender(&self) -> Sender<RpcRawBlock> {
        self.sender.clone()
    }

    pub fn template(&self) -> Arc<Mutex<GetBlockTemplateResponse>> {
        self.template.clone()
    }

    pub fn tx_counter(&self) -> Arc<AtomicUsize> {
        self.tx_counter.clone()
    }
}

#[async_trait]
impl Task for BlockMinerTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let client = self.client.clone();
        let block_count = self.block_count;
        let sender = self.sender();
        let template = self.template();
        let pay_address = self.pay_address.clone();
        let tx_counter = self.tx_counter();
        let dist: Exp<f64> = Exp::new(self.bps as f64).unwrap();
        let comm_delay = self.comm_delay;
        let stopper = self.stopper;
        let task = tokio::spawn(async move {
            warn!("Block miner task starting...");
            for i in 0..block_count {
                // Simulate mining time
                let timeout = max((dist.sample(&mut thread_rng()) * 1000.0) as u64, 1);
                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    _ = sleep(Duration::from_millis(timeout))=> {}
                }

                // Read the most up-to-date block template
                let mut block = template.lock().block.clone();
                // Use index as nonce to avoid duplicate blocks
                block.header.nonce = i as u64;

                let c_template = template.clone();
                let c_client = client.clone();
                let c_pay_address = pay_address.clone();
                tokio::spawn(async move {
                    // We used the current template so let's refetch a new template with new txs
                    let response = c_client.get_block_template(c_pay_address, vec![]).await.unwrap();
                    *c_template.lock() = response;
                });

                let c_sender = sender.clone();
                tx_counter.fetch_add(block.transactions.len() - 1, Ordering::SeqCst);
                tokio::spawn(async move {
                    // Simulate communication delay. TODO: consider adding gaussian noise
                    tokio::time::sleep(Duration::from_millis(comm_delay)).await;
                    let _ = c_sender.send(block).await;
                });
            }
            if stopper == Stopper::Signal {
                stop_signal.trigger.trigger();
            }
            sender.close();
            warn!("Block miner task exited");
        });
        vec![task]
    }
}
