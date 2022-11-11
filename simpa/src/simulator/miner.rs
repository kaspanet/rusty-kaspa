use super::infra::Yield as SimYield;
use super::network::KaspaNetworkSimulator;
use consensus::consensus::test_consensus::TestConsensus;
use consensus::params::Params;
use consensus_core::block::Block;
use hashes::ZERO_HASH;
use rand_distr::{Distribution, Exp};
use std::cell::RefCell;
use std::cmp::max;
use std::ops::Generator;
use std::rc::Rc;
use std::thread::JoinHandle;

pub struct Miner {
    // IDs
    pub(super) mine_id: u64,
    pub(super) recv_id: u64,

    // Consensus
    pub(super) consensus: TestConsensus,
    pub(super) handles: RefCell<Option<Vec<JoinHandle<()>>>>,

    // Config
    lambda: f64,   // In millisecond units
    hashrate: f64, // Relative hashrate of this miner
}

impl Miner {
    pub fn new(mine_id: u64, recv_id: u64, bps: f64, hashrate: f64, params: &Params) -> Self {
        let consensus = TestConsensus::create_from_temp_db(params);
        let handles = consensus.init();
        Self { mine_id, recv_id, lambda: bps / 1000.0, hashrate, consensus, handles: RefCell::new(Some(handles)) }
    }

    fn new_block(&self) -> Block {
        let max_parents = self.consensus.params.max_block_parents as usize;
        let tips = self.consensus.body_tips(); // TEMP
        let mut block = self.consensus.build_block_with_parents_and_transactions(
            ZERO_HASH,
            tips.iter().copied().take(max_parents).collect(),
            vec![],
        );
        block.header.finalize();
        block.to_immutable()
    }

    pub fn mine(self: Rc<Self>, sim: Rc<KaspaNetworkSimulator>) -> impl Generator<Yield = SimYield, Return = ()> {
        move || {
            let dist = Exp::new(1f64 / self.lambda * self.hashrate).unwrap();
            let mut rng = rand::thread_rng();
            loop {
                yield SimYield::Timeout(max(dist.sample(&mut rng) as u64, 1));
                let block = self.new_block();
                sim.broadcast(self.mine_id, block);
            }
        }
    }

    pub fn receive(self: Rc<Self>, sim: Rc<KaspaNetworkSimulator>) -> impl Generator<Yield = SimYield, Return = ()> {
        move || loop {
            yield SimYield::Wait;
            let _ = self.consensus.validate_and_insert_block(sim.env.inbox(self.recv_id));
        }
    }
}
