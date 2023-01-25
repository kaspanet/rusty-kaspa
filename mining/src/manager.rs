// TODO: add integration tests

use crate::{
    block_template::{builder::BlockTemplateBuilder, errors::BuilderError},
    cache::BlockTemplateCache,
    errors::MiningManagerResult,
    mempool::{config::Config, errors::RuleResult, Mempool},
    model::owner_txs::{OwnerSetTransactions, ScriptPublicKeySet},
};
use consensus_core::{
    api::DynConsensus,
    block::BlockTemplate,
    coinbase::MinerData,
    errors::block::RuleError,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutput},
};
use kaspa_core::error;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

pub struct MiningManager {
    block_template_builder: BlockTemplateBuilder,
    block_template_cache: RwLock<BlockTemplateCache>,
    mempool: RwLock<Mempool>,
}

impl MiningManager {
    pub fn new(
        consensus: DynConsensus,
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
    ) -> MiningManager {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(consensus, config, cache_lifetime)
    }

    pub(crate) fn with_config(consensus: DynConsensus, config: Config, cache_lifetime: Option<u64>) -> Self {
        let block_template_builder = BlockTemplateBuilder::new(consensus.clone(), config.maximum_mass_per_block);
        let mempool = RwLock::new(Mempool::new(consensus, config));
        let block_template_cache = RwLock::new(BlockTemplateCache::new(cache_lifetime));
        Self { block_template_builder, block_template_cache, mempool }
    }

    pub fn get_block_template(&self, miner_data: &MinerData) -> MiningManagerResult<BlockTemplate> {
        let cache_read = self.block_template_cache.upgradable_read();
        let immutable_template = cache_read.get_immutable_cached_template();

        // We first try and use a cached template if not expired
        if let Some(immutable_template) = immutable_template {
            drop(cache_read);
            if immutable_template.miner_data == *miner_data {
                return Ok(immutable_template.as_ref().clone());
            }
            // Miner data is new -- make the minimum changes required
            // Note the call returns a modified clone of the cached block template
            let block_template = self.block_template_builder.modify_block_template(miner_data, &immutable_template)?;

            // No point in updating cache since we have no reason to believe this coinbase will be used more
            // than the previous one, and we want to maintain the original template caching time
            return Ok(block_template);
        }

        // Rust rewrite:
        // We avoid passing a mempool ref to blockTemplateBuilder by calling
        // mempool.BlockCandidateTransactions and mempool.RemoveTransactions here.
        // We remove recursion seen in blockTemplateBuilder.BuildBlockTemplate here.
        let mut cache_write = RwLockUpgradableReadGuard::upgrade(cache_read);
        loop {
            let transactions = self.block_candidate_transactions();
            match self.block_template_builder.build_block_template(miner_data, transactions) {
                Ok(block_template) => {
                    let block_template = cache_write.set_immutable_cached_template(block_template);
                    return Ok(block_template.as_ref().clone());
                }
                Err(BuilderError::ConsensusError(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))) => {
                    let mut mempool_write = self.mempool.write();
                    let removal_result = invalid_transactions.iter().try_for_each(|(x, _)| mempool_write.remove_transaction(x, true));
                    drop(mempool_write);
                    if let Err(err) = removal_result {
                        // Original golang comment:
                        // mempool.remove_transactions might return errors in situations that are perfectly fine in this context.
                        // TODO: Once the mempool invariants are clear, this might return an error:
                        // https://github.com/kaspanet/kaspad/issues/1553
                        error!("Error from mempool.remove_transactions: {:?}", err);
                    }
                }
                Err(err) => {
                    return Err(err)?;
                }
            }
        }
    }

    pub(crate) fn block_candidate_transactions(&self) -> Vec<MutableTransaction> {
        self.mempool.read().block_candidate_transactions()
    }

    /// Clears the block template cache, forcing the next call to get_block_template to build a new block template.
    pub fn clear_block_template(&self) {
        self.block_template_cache.write().clear();
    }

    #[cfg(test)]
    pub(crate) fn block_template_builder(&self) -> &BlockTemplateBuilder {
        &self.block_template_builder
    }

    /// validate_and_insert_transaction validates the given transaction, and
    /// adds it to the set of known transactions that have not yet been
    /// added to any block.
    ///
    /// The returned transactions are clones of objects owned by the mempool.
    pub fn validate_and_insert_transaction(
        &self,
        transaction: MutableTransaction,
        is_high_priority: bool,
        allow_orphan: bool,
    ) -> MiningManagerResult<Vec<MutableTransaction>> {
        Ok(self.mempool.write().validate_and_insert_transaction(transaction, is_high_priority, allow_orphan)?)
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

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transactions_by_addresses(
        &self,
        script_public_keys: &ScriptPublicKeySet,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> OwnerSetTransactions {
        self.mempool.read().get_transactions_by_addresses(script_public_keys, include_transaction_pool, include_orphan_pool)
    }

    pub fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        self.mempool.read().transaction_count(include_transaction_pool, include_orphan_pool)
    }

    pub fn handle_new_block_transactions(&self, block_transactions: &[Transaction]) -> MiningManagerResult<Vec<MutableTransaction>> {
        Ok(self.mempool.write().handle_new_block_transactions(block_transactions)?)
    }

    pub fn revalidate_high_priority_transactions(&self) -> RuleResult<Vec<TransactionId>> {
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
