use async_channel::unbounded;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use std::sync::Arc;
use std::thread::JoinHandle;

use super::miner::Miner;

use kaspa_consensus::config::Config;
use kaspa_consensus::consensus::Consensus;
use kaspa_consensus_core::block::Block;
use kaspa_database::utils::{create_permanent_db, create_temp_db, DbLifetime};
use kaspa_utils::sim::Simulation;

type ConsensusWrapper = (Arc<Consensus>, Vec<JoinHandle<()>>, DbLifetime);

pub struct KaspaNetworkSimulator {
    // Internal simulation env
    pub(super) simulation: Simulation<Block>,

    // Consensus instances
    consensuses: Vec<ConsensusWrapper>,

    config: Arc<Config>,        // Consensus config
    bps: f64,                   // Blocks per second
    target_blocks: Option<u64>, // Target simulation blocks
    output_dir: Option<String>, // Possible permanent output directory
}

impl KaspaNetworkSimulator {
    pub fn new(delay: f64, bps: f64, target_blocks: Option<u64>, config: Arc<Config>, output_dir: Option<String>) -> Self {
        Self {
            simulation: Simulation::with_start_time((delay * 1000.0) as u64, config.genesis.timestamp),
            consensuses: Vec::new(),
            bps,
            config,
            target_blocks,
            output_dir,
        }
    }

    pub fn init(&mut self, num_miners: u64, target_txs_per_block: u64) -> &mut Self {
        let secp = secp256k1::Secp256k1::new();
        let mut rng = rand::thread_rng();
        for i in 0..num_miners {
            let (lifetime, db) = if i == 0 && self.output_dir.is_some() {
                create_permanent_db(self.output_dir.clone().unwrap(), num_cpus::get())
            } else {
                create_temp_db()
            };
            let (dummy_notification_sender, _) = unbounded();
            let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
            let consensus =
                Arc::new(Consensus::new(db, self.config.clone(), Default::default(), notification_root, Default::default()));
            let handles = consensus.run_processors();
            let (sk, pk) = secp.generate_keypair(&mut rng);
            let miner_process = Box::new(Miner::new(
                i,
                self.bps,
                1f64 / num_miners as f64,
                sk,
                pk,
                consensus.clone(),
                &self.config,
                target_txs_per_block,
                self.target_blocks,
            ));
            self.simulation.register(i, miner_process);
            self.consensuses.push((consensus, handles, lifetime));
        }
        self
    }

    pub fn run(&mut self, until: u64) -> ConsensusWrapper {
        self.simulation.run(until);
        for (consensus, handles, _) in self.consensuses.drain(1..) {
            consensus.shutdown(handles);
        }
        self.consensuses.pop().unwrap()
    }
}
