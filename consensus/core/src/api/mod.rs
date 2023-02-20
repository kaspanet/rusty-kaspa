use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::{
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        pruning::PruningError,
        tx::TxResult,
    },
    pruning::{PruningPointProof, PruningPointsList},
    trusted::TrustedBlock,
    tx::{MutableTransaction, Transaction},
};

pub type BlockValidationFuture = BoxFuture<'static, BlockProcessResult<BlockStatus>>;

/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError>;

    fn validate_and_insert_block(self: Arc<Self>, block: Block, update_virtual: bool) -> BlockValidationFuture;

    fn validate_and_insert_trusted_block(self: Arc<Self>, tb: TrustedBlock) -> BlockValidationFuture;

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also [`calculated_fee`] is expected to be populated
    fn validate_mempool_transaction_and_populate(self: Arc<Self>, transaction: &mut MutableTransaction) -> TxResult<()>;

    fn calculate_transaction_mass(self: Arc<Self>, transaction: &Transaction) -> u64;

    fn get_virtual_daa_score(self: Arc<Self>) -> u64;

    fn modify_coinbase_payload(self: Arc<Self>, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>>;

    fn validate_pruning_proof(self: Arc<Self>, proof: &PruningPointProof) -> Result<(), PruningError>;

    fn apply_pruning_proof(self: Arc<Self>, proof: PruningPointProof, trusted_set: &[TrustedBlock]);

    fn import_pruning_points(self: Arc<Self>, pruning_points: PruningPointsList);
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
