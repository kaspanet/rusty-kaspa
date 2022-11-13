use std::sync::Arc;
use std::thread::JoinHandle;

use super::infra::Simulation;
use super::miner::Miner;

use consensus::consensus::test_consensus::{create_temp_db, TempDbLifetime};
use consensus::consensus::Consensus;
use consensus::params::Params;
use consensus_core::block::Block;

pub struct KaspaNetworkSimulator {
    // Internal simulation env
    pub(super) simulation: Simulation<Block>,

    // Consensus instances
    consensuses: Vec<(Arc<Consensus>, Vec<JoinHandle<()>>, TempDbLifetime)>,

    params: Params, // Consensus params
    bps: f64,       // Blocks per second
}

impl KaspaNetworkSimulator {
    pub fn new(delay: f64, bps: f64, params: &Params) -> Self {
        Self { simulation: Simulation::new(delay as u64), consensuses: Vec::new(), bps, params: params.clone() }
    }

    pub fn init(&mut self, num_miners: u64) -> &mut Self {
        for i in 0..num_miners {
            let (lifetime, db) = create_temp_db();
            let consensus = Arc::new(Consensus::new(db, &self.params));
            let handles = consensus.init();
            let miner_process = Box::new(Miner::new(i, self.bps, 1f64 / num_miners as f64, consensus.clone()));
            self.simulation.register(i, miner_process);
            self.consensuses.push((consensus, handles, lifetime));
        }
        self
    }

    pub fn run(&mut self, until: u64) {
        self.simulation.run(until);
        for (consensus, handles, _) in self.consensuses.drain(..) {
            consensus.shutdown(handles);
        }
    }
}
