use std::sync::Arc;
use std::thread::JoinHandle;

use super::infra::Simulation;
use super::miner::Miner;

use consensus::consensus::test_consensus::{create_temp_db, TempDbLifetime};
use consensus::consensus::Consensus;
use consensus::constants::perf::PerfParams;
use consensus::params::Params;
use consensus_core::block::Block;

type ConsensusWrapper = (Arc<Consensus>, Vec<JoinHandle<()>>, TempDbLifetime);

pub struct KaspaNetworkSimulator {
    // Internal simulation env
    pub(super) simulation: Simulation<Block>,

    // Consensus instances
    consensuses: Vec<ConsensusWrapper>,

    params: Params,          // Consensus params
    perf_params: PerfParams, // Performance params
    bps: f64,                // Blocks per second
}

impl KaspaNetworkSimulator {
    pub fn new(delay: f64, bps: f64, params: &Params, perf_params: &PerfParams) -> Self {
        Self {
            simulation: Simulation::new((delay * 1000.0) as u64),
            consensuses: Vec::new(),
            bps,
            params: params.clone(),
            perf_params: perf_params.clone(),
        }
    }

    pub fn init(&mut self, num_miners: u64, target_txs_per_block: u64, verbose: bool) -> &mut Self {
        let secp = secp256k1::Secp256k1::new();
        let mut rng = rand::thread_rng();
        for i in 0..num_miners {
            let (lifetime, db) = create_temp_db();
            let consensus = Arc::new(Consensus::with_perf_params(db, &self.params, &self.perf_params));
            let handles = consensus.init();
            let (sk, pk) = secp.generate_keypair(&mut rng);
            let miner_process = Box::new(Miner::new(
                i,
                self.bps,
                1f64 / num_miners as f64,
                sk,
                pk,
                consensus.clone(),
                &self.params,
                target_txs_per_block,
                verbose,
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
