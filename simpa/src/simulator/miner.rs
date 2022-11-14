use super::infra::{Environment, Process, Resumption, Suspension};
use consensus::consensus::Consensus;
use consensus::errors::BlockProcessResult;
use consensus::model::stores::statuses::BlockStatus;
use consensus::model::stores::virtual_state::VirtualStateStoreReader;
use consensus::params::Params;
use consensus_core::block::Block;
use consensus_core::coinbase::MinerData;
use consensus_core::sign::sign;
use consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
};
use consensus_core::utxo::utxo_view::UtxoView;
use kaspa_core::assert_match;
use rand::rngs::ThreadRng;
use rand_distr::{Distribution, Exp};
use std::cmp::max;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct Miner {
    // ID
    pub(super) id: u64,

    // Consensus
    pub(super) consensus: Arc<Consensus>,
    pub(super) params: Params,

    // Miner data
    miner_data: MinerData,
    secret_key: secp256k1::SecretKey,

    // Pending tasks
    futures: Vec<Pin<Box<dyn Future<Output = BlockProcessResult<BlockStatus>>>>>,

    // UTXO data related to this miner
    possible_outpoints: HashSet<TransactionOutpoint>,

    // Rand
    dist: Exp<f64>, // The time interval between Poisson(lambda) events distributes ~Exp(1/lambda)
    rng: ThreadRng,
}

impl Miner {
    pub fn new(
        id: u64,
        bps: f64,
        hashrate: f64,
        sk: secp256k1::SecretKey,
        pk: secp256k1::PublicKey,
        consensus: Arc<Consensus>,
        params: &Params,
    ) -> Self {
        Self {
            id,
            consensus,
            params: params.clone(),
            miner_data: MinerData::new(ScriptPublicKey::new(0, ScriptVec::from_slice(&pk.serialize())), Vec::new()),
            secret_key: sk,
            futures: Vec::new(),
            possible_outpoints: HashSet::new(),
            dist: Exp::new(1f64 / (bps * hashrate)).unwrap(),
            rng: rand::thread_rng(),
        }
    }

    fn build_new_block(&mut self, timestamp: u64) -> Block {
        // Sync on all processed blocks before building the new block
        for fut in self.futures.drain(..) {
            let status = futures::executor::block_on(fut).unwrap();
            assert_match!(status, BlockStatus::StatusUTXOPendingVerification | BlockStatus::StatusUTXOValid);
        }
        let txs = self.build_txs();
        let nonce = self.id;
        let block_template = self.consensus.build_block_template(timestamp, nonce, self.miner_data.clone(), txs);
        block_template.block.to_immutable()
    }

    fn build_txs(&mut self) -> Vec<Transaction> {
        let virtual_state = self.consensus.virtual_processor.virtual_state_store.read().get().unwrap();
        let mut txs = Vec::new();
        let mut spent_outpoints = HashSet::new();
        for &outpoint in self.possible_outpoints.iter() {
            if txs.len() >= 150 {
                break;
            }
            if let Some(entry) = self.consensus.virtual_processor.virtual_utxo_store.get(&outpoint) {
                if entry.amount < 2
                    || (entry.is_coinbase
                        && (virtual_state.daa_score as i64 - entry.block_daa_score as i64) <= self.params.coinbase_maturity as i64)
                {
                    continue;
                }
                let unsigned_tx = Transaction::new(
                    0,
                    vec![TransactionInput::new(outpoint, vec![], 0, 0)],
                    if self.possible_outpoints.len() < 10_000 && entry.amount > 4 {
                        vec![
                            TransactionOutput::new(entry.amount / 2, self.miner_data.script_public_key.clone()),
                            TransactionOutput::new(entry.amount / 2 - 1, self.miner_data.script_public_key.clone()),
                        ]
                    } else {
                        vec![TransactionOutput::new(entry.amount - 1, self.miner_data.script_public_key.clone())]
                    },
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                );
                let signed_tx = sign(&PopulatedTransaction::new(&unsigned_tx, vec![entry]), self.secret_key.secret_bytes());
                txs.push(signed_tx);
                spent_outpoints.insert(outpoint);
            }
        }
        self.possible_outpoints.retain(|o| !spent_outpoints.contains(o));
        txs
    }

    pub fn mine(&mut self, env: &mut Environment<Block>) -> Suspension {
        let block = self.build_new_block(env.now());
        env.broadcast(self.id, block);
        self.sample_mining_interval()
    }

    fn sample_mining_interval(&mut self) -> Suspension {
        Suspension::Timeout(max((self.dist.sample(&mut self.rng) * 1000.0) as u64, 1))
    }

    fn process_block(&mut self, block: Block) -> Suspension {
        for tx in block.transactions.iter() {
            for (i, output) in tx.outputs.iter().enumerate() {
                if output.script_public_key.eq(&self.miner_data.script_public_key) {
                    self.possible_outpoints.insert(TransactionOutpoint::new(tx.id(), i as u32));
                }
            }
        }
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
