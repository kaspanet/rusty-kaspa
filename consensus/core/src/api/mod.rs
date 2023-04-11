use futures_util::future::BoxFuture;
use kaspa_muhash::MuHash;
use std::sync::Arc;

use crate::{
    acceptance_data::AcceptanceData,
    block::{Block, BlockInfo, BlockRelations, BlockTemplate},
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
    sync_info::SyncInfo,
    trusted::{TrustedBlock, TrustedGhostdagData, TrustedHeader},
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet, ChainPath,
};
use kaspa_hashes::Hash;
pub type BlockValidationFuture = BoxFuture<'static, BlockProcessResult<BlockStatus>>;

/// Abstracts the consensus external API
#[allow(unused_variables)]
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        unimplemented!()
    }

    fn validate_and_insert_block(&self, block: Block, update_virtual: bool) -> BlockValidationFuture {
        unimplemented!()
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFuture {
        unimplemented!()
    }

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also [`transaction.calculated_fee`] is expected to be populated
    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        unimplemented!()
    }

    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64 {
        unimplemented!()
    }

    fn get_virtual_daa_score(&self) -> u64 {
        unimplemented!()
    }

    fn get_virtual_merge_depth_root(&self) -> Option<Hash> {
        unimplemented!()
    }

    fn get_sink(&self) -> Option<Hash> {
        unimplemented!()
    }

    fn get_sink_timestamp(&self) -> Option<u64> {
        unimplemented!()
    }

    fn get_sync_info(&self) -> SyncInfo {
        unimplemented!()
    }

    /// Returns whether this consensus is considered synced or close to being synced.
    ///
    /// This info is used to determine if it's ok to use a block template from this node for mining purposes.
    fn is_nearly_synced(&self) -> bool {
        unimplemented!()
    }

    fn get_virtual_chain_from_block(&self, hash: Hash) -> Option<ChainPath> {
        unimplemented!()
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
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

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        unimplemented!()
    }

    fn validate_pruning_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) {
        unimplemented!()
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) {
        unimplemented!()
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        unimplemented!()
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: Hash, imported_utxo_multiset: &mut MuHash) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn header_exists(&self, hash: Hash) -> bool {
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

    fn get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        unimplemented!()
    }

    fn create_headers_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>> {
        unimplemented!()
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        unimplemented!()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> Arc<(Vec<Hash>, Vec<TrustedHeader>, Vec<TrustedGhostdagData>)> {
        unimplemented!()
    }

    fn get_block(&self, hash: Hash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_block_info(&self, hash: Hash) -> ConsensusResult<BlockInfo> {
        unimplemented!()
    }

    fn get_block_relations(&self, hash: Hash) -> Option<BlockRelations> {
        unimplemented!()
    }

    fn get_block_children(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
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

    fn pruning_point(&self) -> Option<Hash> {
        unimplemented!()
    }
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
