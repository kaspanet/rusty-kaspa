use futures_util::future::BoxFuture;
use std::sync::Arc;
use std::error::Error;
use async_std::channel::Receiver;

use crate::{
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    errors::{
        block::{BlockProcessResult, RuleError},
        tx::TxResult,
    },
    notify::ConsensusNotification,
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
};
use hashes::Hash;
/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError>;

    fn validate_and_insert_block(
        self: Arc<Self>,
        block: Block,
        update_virtual: bool,
    ) -> BoxFuture<'static, BlockProcessResult<BlockStatus>>;

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also [`calculated_fee`] is expected to be populated
    fn validate_mempool_transaction_and_populate(self: Arc<Self>, transaction: &mut MutableTransaction) -> TxResult<()>;

    fn calculate_transaction_mass(self: Arc<Self>, transaction: &Transaction) -> u64;

    fn get_virtual_daa_score(self: Arc<Self>) -> u64;

    fn get_virtual_state_tips(self: Arc<Self>) -> Vec<Hash>;

    fn get_virtual_utxos(
        self: Arc<Self>,
        from_outpoint: TransactionOutpoint,
        chunk_size: usize,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)>;

}

pub type DynConsensus = Arc<dyn ConsensusApi>;
