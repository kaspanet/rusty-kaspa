use crate::{
    common::daemon::ClientManager,
    tasks::{
        block::{miner::BlockMinerTask, submitter::BlockSubmitterTask, template_receiver::BlockTemplateReceiverTask},
        Stopper, Task,
    },
};
use async_trait::async_trait;
use itertools::chain;
use kaspa_addresses::Address;
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::debug;
use kaspa_utils::triggers::SingleTrigger;
use rand::thread_rng;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct MinerGroupTask {
    submitter: Arc<BlockSubmitterTask>,
    receiver: Arc<BlockTemplateReceiverTask>,
    miner: Arc<BlockMinerTask>,
}

impl MinerGroupTask {
    pub fn new(submitter: Arc<BlockSubmitterTask>, receiver: Arc<BlockTemplateReceiverTask>, miner: Arc<BlockMinerTask>) -> Self {
        Self { submitter, receiver, miner }
    }

    pub async fn build(
        network: NetworkId,
        client_manager: Arc<ClientManager>,
        submitter_pool_size: usize,
        bps: u64,
        block_count: usize,
        stopper: Stopper,
    ) -> Arc<Self> {
        // Block submitter
        let submitter = BlockSubmitterTask::build(client_manager.clone(), submitter_pool_size, stopper).await;

        // Mining key and address
        let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
        let pay_address =
            Address::new(network.network_type().into(), kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
        debug!("Generated private key {} and address {}", sk.display_secret(), pay_address);

        // Block template receiver
        let client = Arc::new(client_manager.new_client().await);
        let receiver = BlockTemplateReceiverTask::build(client.clone(), pay_address.clone(), stopper).await;

        // Miner
        let miner =
            BlockMinerTask::build(client, bps, block_count, submitter.sender(), receiver.template(), pay_address, stopper).await;

        Arc::new(Self::new(submitter, receiver, miner))
    }
}

#[async_trait]
impl Task for MinerGroupTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        chain![
            self.submitter.start(stop_signal.clone()),
            self.receiver.start(stop_signal.clone()),
            self.miner.start(stop_signal.clone())
        ]
        .collect()
    }
}
