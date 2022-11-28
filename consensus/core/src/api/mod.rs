use std::sync::Arc;

use crate::{
    stubs::{BlockTemplate, MinerData},
    tx::Transaction,
};

/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> BlockTemplate;
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
