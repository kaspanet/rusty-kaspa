use consensus_core::{
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    constants::BLOCK_VERSION,
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        consensus::ConsensusError,
        pruning::PruningImportResult,
        tx::{TxResult, TxRuleError},
    },
    header::Header,
    mass::transaction_estimated_serialized_size,
    merkle::calc_hash_merkle_root,
    trusted::TrustedBlock,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use futures_util::future::BoxFuture;
use hashes::ZERO_HASH;
use muhash::MuHash;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc, time::SystemTime, unimplemented};

use super::coinbase_mock::CoinbaseManagerMock;

pub(crate) struct ConsensusMock {
    transactions: RwLock<HashMap<TransactionId, Arc<Transaction>>>,
    statuses: RwLock<HashMap<TransactionId, TxResult<()>>>,
    utxos: RwLock<UtxoCollection>,
}

impl ConsensusMock {
    pub(crate) fn new() -> Self {
        Self {
            transactions: RwLock::new(HashMap::default()),
            statuses: RwLock::new(HashMap::default()),
            utxos: RwLock::new(HashMap::default()),
        }
    }

    pub(crate) fn set_status(&self, transaction_id: TransactionId, status: TxResult<()>) {
        self.statuses.write().insert(transaction_id, status);
    }

    pub(crate) fn add_transaction(&self, transaction: Transaction, block_daa_score: u64) {
        let transaction = MutableTransaction::from_tx(transaction);
        let mut transactions = self.transactions.write();
        let mut utxos = self.utxos.write();

        // Remove the spent UTXOs
        transaction.tx.inputs.iter().for_each(|x| {
            utxos.remove(&x.previous_outpoint);
        });
        // Create the new UTXOs
        transaction.tx.outputs.iter().enumerate().for_each(|(i, x)| {
            utxos.insert(
                TransactionOutpoint::new(transaction.id(), i as u32),
                UtxoEntry::new(x.value, x.script_public_key.clone(), block_daa_score, transaction.tx.is_coinbase()),
            );
        });
        // Register the transaction
        transactions.insert(transaction.id(), transaction.tx);
    }

    pub(crate) fn can_finance_transaction(&self, transaction: &MutableTransaction) -> bool {
        let utxos = self.utxos.read();
        for outpoint in transaction.missing_outpoints() {
            if !utxos.contains_key(&outpoint) {
                return false;
            }
        }
        true
    }
}

impl ConsensusApi for ConsensusMock {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, mut txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        let coinbase_manager = CoinbaseManagerMock::new();
        let coinbase = coinbase_manager.expected_coinbase_transaction(miner_data.clone());
        txs.insert(0, coinbase.tx);
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let hash_merkle_root = calc_hash_merkle_root(txs.iter());
        let header = Header::new(
            BLOCK_VERSION,
            vec![],
            hash_merkle_root,
            ZERO_HASH,
            ZERO_HASH,
            now,
            123456789u32,
            0,
            0,
            0.into(),
            0,
            ZERO_HASH,
        );
        let mutable_block = MutableBlock::new(header, txs);

        Ok(BlockTemplate::new(mutable_block, miner_data, coinbase.has_red_reward, now))
    }

    fn validate_and_insert_block(
        self: Arc<Self>,
        _block: Block,
        _update_virtual: bool,
    ) -> BoxFuture<'static, BlockProcessResult<BlockStatus>> {
        unimplemented!()
    }

    fn validate_and_insert_trusted_block(self: Arc<Self>, _tb: TrustedBlock) -> BoxFuture<'static, BlockProcessResult<BlockStatus>> {
        unimplemented!()
    }

    fn validate_mempool_transaction_and_populate(self: Arc<Self>, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        // If a predefined status was registered to simulate an error, return it right away
        if let Some(status) = self.statuses.read().get(&mutable_tx.id()) {
            if status.is_err() {
                return status.clone();
            }
        }
        let utxos = self.utxos.read();
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            // Keep existing entries
            if mutable_tx.entries[i].is_some() {
                continue;
            }
            // Try add missing entries
            if let Some(entry) = utxos.get(&mutable_tx.tx.inputs[i].previous_outpoint) {
                mutable_tx.entries[i] = Some(entry.clone());
            } else {
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        // At this point we know all UTXO entries are populated, so we can safely calculate the fee
        let total_in: u64 = mutable_tx.entries.iter().map(|x| x.as_ref().unwrap().amount).sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|x| x.value).sum();
        let calculated_fee = total_in - total_out;
        mutable_tx.calculated_fee = Some(calculated_fee);
        Ok(())
    }

    fn calculate_transaction_mass(self: Arc<Self>, transaction: &Transaction) -> u64 {
        if transaction.is_coinbase() {
            0
        } else {
            transaction_estimated_serialized_size(transaction)
        }
    }

    fn get_virtual_daa_score(self: Arc<Self>) -> u64 {
        0
    }

    fn modify_coinbase_payload(self: Arc<Self>, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        let coinbase_manager = CoinbaseManagerMock::new();
        Ok(coinbase_manager.modify_coinbase_payload(payload, miner_data))
    }

    fn validate_pruning_proof(self: Arc<Self>, _proof: &consensus_core::pruning::PruningPointProof) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn apply_pruning_proof(self: Arc<Self>, _proof: consensus_core::pruning::PruningPointProof, _trusted_set: &[TrustedBlock]) {
        unimplemented!()
    }

    fn import_pruning_points(self: Arc<Self>, _pruning_points: consensus_core::pruning::PruningPointsList) {
        unimplemented!()
    }

    fn get_virtual_state_tips(self: Arc<Self>) -> Vec<hashes::Hash> {
        unimplemented!()
    }

    fn get_virtual_utxos(
        self: Arc<Self>,
        _from_outpoint: Option<TransactionOutpoint>,
        _limit: usize,
        _skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        unimplemented!()
    }

    fn header_exists(self: Arc<Self>, _hash: hashes::Hash) -> bool {
        unimplemented!()
    }

    fn is_chain_ancestor_of(self: Arc<Self>, _low: hashes::Hash, _high: hashes::Hash) -> Result<bool, ConsensusError> {
        unimplemented!()
    }

    fn get_hashes_between(
        self: Arc<Self>,
        _low: hashes::Hash,
        _high: hashes::Hash,
        _max_blocks: usize,
    ) -> consensus_core::errors::consensus::ConsensusResult<(Vec<hashes::Hash>, hashes::Hash)> {
        unimplemented!()
    }

    fn get_header(self: Arc<Self>, _hash: hashes::Hash) -> consensus_core::errors::consensus::ConsensusResult<Arc<Header>> {
        unimplemented!()
    }

    fn append_imported_pruning_point_utxos(
        &self,
        _utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)],
        _current_multiset: &mut MuHash,
    ) {
        unimplemented!()
    }

    fn import_pruning_point_utxo_set(
        &self,
        _new_pruning_point: hashes::Hash,
        _imported_utxo_multiset: &mut MuHash,
    ) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn get_pruning_point_proof(self: Arc<Self>) -> Arc<consensus_core::pruning::PruningPointProof> {
        unimplemented!()
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        unimplemented!()
    }

    fn get_pruning_point_anticone_and_trusted_data(
        &self,
    ) -> Arc<(Vec<hashes::Hash>, Vec<consensus_core::trusted::TrustedHeader>, Vec<consensus_core::trusted::TrustedGhostdagData>)> {
        unimplemented!()
    }

    fn get_block(&self, _hash: hashes::Hash) -> consensus_core::errors::consensus::ConsensusResult<Block> {
        unimplemented!()
    }

    fn create_headers_selected_chain_block_locator(
        &self,
        _low: Option<hashes::Hash>,
        _high: Option<hashes::Hash>,
    ) -> consensus_core::errors::consensus::ConsensusResult<Vec<hashes::Hash>> {
        unimplemented!()
    }

    fn get_pruning_point_utxos(
        self: Arc<Self>,
        _expected_pruning_point: hashes::Hash,
        _from_outpoint: Option<TransactionOutpoint>,
        _chunk_size: usize,
        _skip_first: bool,
    ) -> consensus_core::errors::consensus::ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        unimplemented!()
    }
}
