use super::infra::{Environment, Process, Resumption, Suspension};
use consensus::consensus::test_consensus::TestConsensus;
use consensus_core::block::Block;
use hashes::ZERO_HASH;
use rand::rngs::ThreadRng;
use rand_distr::{Distribution, Exp};
use std::cmp::max;
use std::sync::Arc;

pub struct Miner {
    // ID
    pub(super) id: u64,

    // Consensus
    pub(super) consensus: Arc<TestConsensus>,

    // Rand
    dist: Exp<f64>, // The time interval between Poisson(lambda) events distributes ~Exp(1/lambda)
    rng: ThreadRng,
}

impl Miner {
    pub fn new(id: u64, bps: f64, hashrate: f64, consensus: Arc<TestConsensus>) -> Self {
        let lambda = bps / 1000.0;
        Self { id, consensus, dist: Exp::new(1f64 / (lambda * hashrate)).unwrap(), rng: rand::thread_rng() }
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

    pub fn mine(&mut self, env: &mut Environment<Block>) -> Suspension {
        let block = self.new_block();
        env.broadcast(self.id, block);
        self.sample_mining_interval()
    }

    pub fn sample_mining_interval(&mut self) -> Suspension {
        Suspension::Timeout(max(self.dist.sample(&mut self.rng) as u64, 1))
    }

    fn process_block(&mut self, block: Block) -> Suspension {
        let _ = self.consensus.validate_and_insert_block(block);
        Suspension::Idle
    }
}

impl Process<Block> for Miner {
    fn resume(&mut self, resumption: Resumption<Block>, env: &mut Environment<Block>) -> Suspension {
        match resumption {
            Resumption::Initial => self.sample_mining_interval(),
            Resumption::Scheduled => self.mine(env),
            Resumption::Message(block) => self.process_block(block),
        }
    }
}
