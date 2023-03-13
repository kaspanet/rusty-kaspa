use futures_util::future::BoxFuture;
use muhash::MuHash;
use std::sync::Arc;

use crate::{
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        consensus::ConsensusResult,
        pruning::PruningImportResult,
        tx::TxResult,
    },
    header::Header,
    pruning::{PruningPointProof, PruningPointsList},
    trusted::{TrustedBlock, TrustedGhostdagData, TrustedHeader},
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet,
};
use hashes::Hash;
pub type BlockValidationFuture = BoxFuture<'static, BlockProcessResult<BlockStatus>>;

/// Abstracts the consensus external API
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError>;

    fn validate_and_insert_block(&self, block: Block, update_virtual: bool) -> BlockValidationFuture;

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFuture;

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also [`calculated_fee`] is expected to be populated
    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()>;

    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64;

    fn get_virtual_daa_score(&self) -> u64;

    fn get_virtual_merge_depth_root(&self) -> Option<Hash>;

    fn get_sink_timestamp(&self) -> Option<u64>;

    fn get_virtual_parents(&self) -> BlockHashSet;

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)>;

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>>;

    fn validate_pruning_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()>;

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]);

    fn import_pruning_points(&self, pruning_points: PruningPointsList);

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash);

    fn import_pruning_point_utxo_set(&self, new_pruning_point: Hash, imported_utxo_multiset: &mut MuHash) -> PruningImportResult<()>;

    fn header_exists(&self, hash: Hash) -> bool;

    fn is_chain_ancestor_of(&self, low: Hash, high: Hash) -> ConsensusResult<bool>;

    fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)>;

    fn get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>>;

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof>;

    fn create_headers_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>>;

    fn pruning_point_headers(&self) -> Vec<Arc<Header>>;

    fn get_pruning_point_anticone_and_trusted_data(&self) -> Arc<(Vec<Hash>, Vec<TrustedHeader>, Vec<TrustedGhostdagData>)>;

    fn get_block(&self, hash: Hash) -> ConsensusResult<Block>;

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: Hash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>>;

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus>;

    fn get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>>;

    fn pruning_point(&self) -> Option<Hash>;
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
