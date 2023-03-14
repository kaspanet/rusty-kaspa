use consensus_core::{
    api::ConsensusApi,
    block::BlockTemplate,
    coinbase::MinerData,
    errors::{block::RuleError, coinbase::CoinbaseResult, tx::TxResult},
    tx::{MutableTransaction, Transaction},
};

/// Internal trait for abstracting the consensus dependency
pub trait ConsensusMiningContext {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError>;
    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()>;
    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64;
    fn get_virtual_daa_score(&self) -> u64;
    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>>;
}

impl ConsensusMiningContext for dyn ConsensusApi {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        self.build_block_template(miner_data, txs)
    }

    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        self.validate_mempool_transaction_and_populate(transaction)
    }

    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64 {
        self.calculate_transaction_mass(transaction)
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.get_virtual_daa_score()
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        self.modify_coinbase_payload(payload, miner_data)
    }
}
