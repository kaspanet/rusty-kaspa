use futures_util::future::BoxFuture;
use kaspa_muhash::MuHash;
use std::sync::Arc;

use crate::{
    acceptance_data::AcceptanceData,
    block::{Block, BlockTemplate, TemplateBuildMode, TemplateTransactionSelector, VirtualStateApproxId},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    daa_score_timestamp::DaaScoreTimestamp,
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        consensus::ConsensusResult,
        pruning::PruningImportResult,
        tx::TxResult,
    },
    header::Header,
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet, BlueWorkType, ChainPath,
};
use kaspa_hashes::Hash;

pub use self::stats::{BlockCount, ConsensusStats};

pub mod counters;
pub mod stats;

pub type BlockValidationFuture = BoxFuture<'static, BlockProcessResult<BlockStatus>>;

/// A struct returned by consensus for block validation processing calls
pub struct BlockValidationFutures {
    /// A future triggered when block processing is completed (header and body processing)
    pub block_task: BlockValidationFuture,

    /// A future triggered when DAG state which included this block has been processed by the virtual processor
    /// (exceptions are header-only blocks and trusted blocks which have the future completed before virtual
    /// processing along with the [`block_task`])
    pub virtual_state_task: BlockValidationFuture,
}

/// Abstracts the consensus external API
#[allow(unused_variables)]
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        unimplemented!()
    }

    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        unimplemented!()
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        unimplemented!()
    }

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also [`transaction.calculated_fee`] is expected to be populated.
    fn validate_mempool_transaction(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        unimplemented!()
    }

    /// Populates the mempool transactions with maximally found UTXO entry data and proceeds to full transactions
    /// validation if all are found. If validation is successful, also [`transaction.calculated_fee`] is expected to be populated.
    fn validate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        unimplemented!()
    }

    /// Populates the mempool transaction with maximally found UTXO entry data.
    fn populate_mempool_transaction(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        unimplemented!()
    }

    /// Populates the mempool transactions with maximally found UTXO entry data.
    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        unimplemented!()
    }

    fn calculate_transaction_compute_mass(&self, transaction: &Transaction) -> u64 {
        unimplemented!()
    }

    fn calculate_transaction_storage_mass(&self, transaction: &MutableTransaction) -> Option<u64> {
        unimplemented!()
    }

    /// Returns an aggregation of consensus stats. Designed to be a fast call.
    fn get_stats(&self) -> ConsensusStats {
        unimplemented!()
    }

    fn get_virtual_daa_score(&self) -> u64 {
        unimplemented!()
    }

    fn get_virtual_bits(&self) -> u32 {
        unimplemented!()
    }

    fn get_virtual_past_median_time(&self) -> u64 {
        unimplemented!()
    }

    fn get_virtual_merge_depth_root(&self) -> Option<Hash> {
        unimplemented!()
    }

    /// Returns the `BlueWork` threshold at which blocks with lower or equal blue work are considered
    /// to be un-mergeable by current virtual state.
    /// (Note: in some rare cases when the node is unsynced the function might return zero as the threshold)
    fn get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        unimplemented!()
    }

    fn get_sink(&self) -> Hash {
        unimplemented!()
    }

    fn get_sink_timestamp(&self) -> u64 {
        unimplemented!()
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        unimplemented!()
    }

    /// source refers to the earliest block from which the current node has full header & block data  
    fn get_source(&self) -> Hash {
        unimplemented!()
    }

    fn estimate_block_count(&self) -> BlockCount {
        unimplemented!()
    }

    /// Returns whether this consensus is considered synced or close to being synced.
    ///
    /// This info is used to determine if it's ok to use a block template from this node for mining purposes.
    fn is_nearly_synced(&self) -> bool {
        unimplemented!()
    }

    fn get_virtual_chain_from_block(&self, hash: Hash) -> ConsensusResult<ChainPath> {
        unimplemented!()
    }

    fn get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        unimplemented!()
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        unimplemented!()
    }

    fn get_virtual_parents_len(&self) -> usize {
        unimplemented!()
    }

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        unimplemented!()
    }

    fn get_tips(&self) -> Vec<Hash> {
        unimplemented!()
    }

    fn get_tips_len(&self) -> usize {
        unimplemented!()
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        unimplemented!()
    }

    fn validate_pruning_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) {
        unimplemented!()
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        unimplemented!()
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: Hash, imported_utxo_multiset: MuHash) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn is_chain_ancestor_of(&self, low: Hash, high: Hash) -> ConsensusResult<bool> {
        unimplemented!()
    }

    fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)> {
        unimplemented!()
    }

    fn get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>> {
        unimplemented!()
    }

    fn get_headers_selected_tip(&self) -> Hash {
        unimplemented!()
    }

    /// Returns the antipast of block `hash` from the POV of `context`, i.e. `antipast(hash) ∩ past(context)`.
    /// Since this might be an expensive operation for deep blocks, we allow the caller to specify a limit
    /// `max_traversal_allowed` on the maximum amount of blocks to traverse for obtaining the answer
    fn get_antipast_from_pov(&self, hash: Hash, context: Hash, max_traversal_allowed: Option<u64>) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    /// Returns the anticone of block `hash` from the POV of `virtual`
    fn get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        unimplemented!()
    }

    fn create_virtual_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn create_block_locator_from_pruning_point(&self, high: Hash, limit: usize) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        unimplemented!()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        unimplemented!()
    }

    fn get_block(&self, hash: Hash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_ghostdag_data(&self, hash: Hash) -> ConsensusResult<ExternalGhostdagData> {
        unimplemented!()
    }

    fn get_block_children(&self, hash: Hash) -> Option<Vec<Hash>> {
        unimplemented!()
    }

    fn get_block_parents(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        unimplemented!()
    }

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        unimplemented!()
    }

    fn get_block_acceptance_data(&self, hash: Hash) -> ConsensusResult<Arc<AcceptanceData>> {
        unimplemented!()
    }

    /// Returns acceptance data for a set of blocks belonging to the selected parent chain.
    ///
    /// See `self::get_virtual_chain`
    fn get_blocks_acceptance_data(&self, hashes: &[Hash]) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        unimplemented!()
    }

    fn is_chain_block(&self, hash: Hash) -> ConsensusResult<bool> {
        unimplemented!()
    }

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: Hash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        unimplemented!()
    }

    fn get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn pruning_point(&self) -> Hash {
        unimplemented!()
    }

    // TODO: Delete this function once there's no need for go-kaspad backward compatibility.
    fn get_daa_window(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    // TODO: Think of a better name.
    // TODO: Delete this function once there's no need for go-kaspad backward compatibility.
    fn get_trusted_block_associated_ghostdag_data_block_hashes(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn estimate_network_hashes_per_second(&self, start_hash: Option<Hash>, window_size: usize) -> ConsensusResult<u64> {
        unimplemented!()
    }

    fn validate_pruning_points(&self) -> ConsensusResult<()> {
        unimplemented!()
    }

    fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        unimplemented!()
    }

    fn creation_timestamp(&self) -> u64 {
        unimplemented!()
    }

    fn finality_point(&self) -> Hash {
        unimplemented!()
    }
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
