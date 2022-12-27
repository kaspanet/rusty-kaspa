use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::{
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    tx::{MutableTransaction, Transaction},
};

use self::error::ConsensusError;

pub mod error;

/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, ConsensusError>;

    fn validate_and_insert_block(
        self: Arc<Self>,
        block: Block,
        update_virtual: bool,
    ) -> BoxFuture<'static, Result<BlockStatus, ConsensusError>>;

    fn validate_mempool_transaction_and_populate(self: Arc<Self>, transaction: &mut MutableTransaction) -> Result<(), ConsensusError>;

    fn calculate_transaction_mass(self: Arc<Self>, transaction: &Transaction) -> u64;
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
