use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::{
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    tx::Transaction,
};

/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> BlockTemplate;

    fn validate_and_insert_block(
        self: Arc<Self>,
        block: Block,
        update_virtual: bool,
    ) -> BoxFuture<'static, Result<BlockStatus, String>>;
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
