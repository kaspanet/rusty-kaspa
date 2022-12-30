use crate::{
    block_template::builder::BlockTemplateBuilder,
    mempool::{errors::RuleResult, Mempool},
};
use consensus_core::{
    api::DynConsensus,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutput},
};
use parking_lot::RwLock;

pub struct MiningManager {
    _block_template_builder: BlockTemplateBuilder,
    mempool: RwLock<Mempool>,
}

impl MiningManager {
    pub fn new(
        consensus: DynConsensus,
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
    ) -> MiningManager {
        let block_template_builder = BlockTemplateBuilder::new();
        let mempool = RwLock::new(Mempool::new(consensus, target_time_per_block, relay_non_std_transactions, max_block_mass));
        Self { _block_template_builder: block_template_builder, mempool }
    }

    pub(crate) fn _block_template_builder(&self) -> &BlockTemplateBuilder {
        &self._block_template_builder
    }

    /// validate_and_insert_transaction validates the given transaction, and
    /// adds it to the set of known transactions that have not yet been
    /// added to any block.
    ///
    /// The returned transactions are clones of objects owned by the mempool.
    pub fn validate_and_insert_transaction(
        &mut self,
        transaction: MutableTransaction,
        is_high_priority: bool,
        allow_orphan: bool,
    ) -> RuleResult<Vec<MutableTransaction>> {
        self.mempool.write().validate_and_insert_transaction(transaction, is_high_priority, allow_orphan)
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transaction(
        &self,
        transaction_id: &TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> Option<MutableTransaction> {
        self.mempool.read().get_transaction(transaction_id, include_transaction_pool, include_orphan_pool)
    }

    pub fn get_all_transactions(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        self.mempool.read().get_all_transactions(include_transaction_pool, include_orphan_pool)
    }

    pub fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        self.mempool.read().transaction_count(include_transaction_pool, include_orphan_pool)
    }

    pub fn handle_new_block_transactions(&self, block_transactions: &[Transaction]) -> RuleResult<Vec<MutableTransaction>> {
        self.mempool.write().handle_new_block_transactions(block_transactions)
    }

    pub fn revalidate_high_priority_transactions(&self) -> RuleResult<Vec<MutableTransaction>> {
        self.mempool.write().revalidate_high_priority_transactions()
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    pub fn is_transaction_output_dust(&self, transaction_output: &TransactionOutput) -> bool {
        self.mempool.read().is_transaction_output_dust(transaction_output)
    }
}
