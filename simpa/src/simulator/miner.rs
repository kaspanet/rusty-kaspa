use super::infra::{Environment, Process, Resumption, Suspension};
use consensus::consensus::Consensus;
use consensus::errors::BlockProcessResult;
use consensus::model::stores::statuses::BlockStatus;
use consensus_core::block::Block;
use consensus_core::coinbase::MinerData;
use consensus_core::tx::{ScriptPublicKey, ScriptVec};
use rand::rngs::ThreadRng;
use rand_distr::{Distribution, Exp};
use std::cmp::max;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct Miner {
    // ID
    pub(super) id: u64,

    // Consensus
    pub(super) consensus: Arc<Consensus>,

    // Miner data
    miner_data: MinerData,

    // Pending tasks
    futures: Vec<Pin<Box<dyn Future<Output = BlockProcessResult<BlockStatus>>>>>,

    // Rand
    dist: Exp<f64>, // The time interval between Poisson(lambda) events distributes ~Exp(1/lambda)
    rng: ThreadRng,
}

impl Miner {
    pub fn new(id: u64, bps: f64, hashrate: f64, consensus: Arc<Consensus>) -> Self {
        Self {
            id,
            consensus,
            miner_data: MinerData::new(ScriptPublicKey::new(0, ScriptVec::from_slice(&id.to_le_bytes())), Vec::new()), // TODO: real script pub key
            futures: Vec::new(),
            dist: Exp::new(1f64 / (bps * hashrate)).unwrap(),
            rng: rand::thread_rng(),
        }
    }

    fn new_block(&mut self, timestamp: u64) -> Block {
        // Sync on all before building the new block
        for fut in self.futures.drain(..) {
            futures::executor::block_on(fut).unwrap();
        }
        let nonce = self.id;
        let block_template = self.consensus.build_block_template(timestamp, nonce, self.miner_data.clone());
        block_template.block
    }

    pub fn mine(&mut self, env: &mut Environment<Block>) -> Suspension {
        let block = self.new_block(env.now());
        env.broadcast(self.id, block);
        self.sample_mining_interval()
    }

    pub fn sample_mining_interval(&mut self) -> Suspension {
        Suspension::Timeout(max((self.dist.sample(&mut self.rng) * 1000.0) as u64, 1))
    }

    fn process_block(&mut self, block: Block) -> Suspension {
        self.futures.push(Box::pin(self.consensus.validate_and_insert_block(block)));
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
