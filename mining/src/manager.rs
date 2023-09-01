// TODO: add integration tests

use std::sync::Arc;

use crate::{
    block_template::{builder::BlockTemplateBuilder, errors::BuilderError},
    cache::BlockTemplateCache,
    errors::MiningManagerResult,
    mempool::{
        config::Config,
        model::tx::MempoolTransaction,
        populate_entries_and_try_validate::{validate_mempool_transaction_and_populate, validate_mempool_transactions_in_parallel},
        tx::{Orphan, Priority},
        Mempool,
    },
    model::{
        candidate_tx::CandidateTransaction,
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
        txs_stager::TransactionsStagger,
    },
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    block::BlockTemplate,
    coinbase::MinerData,
    errors::block::RuleError as BlockRuleError,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutput},
};
use kaspa_consensusmanager::{spawn_blocking, ConsensusProxy};
use kaspa_core::{debug, error, warn};
use kaspa_mining_errors::mempool::RuleError;
use parking_lot::{Mutex, RwLock};

pub struct MiningManager {
    block_template_builder: BlockTemplateBuilder,
    block_template_cache: Mutex<BlockTemplateCache>,
    pub(crate) mempool: RwLock<Mempool>,
}

impl MiningManager {
    pub fn new(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
    ) -> Self {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(config, cache_lifetime)
    }

    pub(crate) fn with_config(config: Config, cache_lifetime: Option<u64>) -> Self {
        let block_template_builder = BlockTemplateBuilder::new(config.maximum_mass_per_block);
        let mempool = RwLock::new(Mempool::new(config));
        let block_template_cache = Mutex::new(BlockTemplateCache::new(cache_lifetime));
        Self { block_template_builder, block_template_cache, mempool }
    }

    pub fn get_block_template(&self, consensus: &dyn ConsensusApi, miner_data: &MinerData) -> MiningManagerResult<BlockTemplate> {
        let mut cache_lock = self.block_template_cache.lock();
        let immutable_template = cache_lock.get_immutable_cached_template();

        // We first try and use a cached template if not expired
        if let Some(immutable_template) = immutable_template {
            drop(cache_lock);
            if immutable_template.miner_data == *miner_data {
                return Ok(immutable_template.as_ref().clone());
            }
            // Miner data is new -- make the minimum changes required
            // Note the call returns a modified clone of the cached block template
            let block_template = self.block_template_builder.modify_block_template(consensus, miner_data, &immutable_template)?;

            // No point in updating cache since we have no reason to believe this coinbase will be used more
            // than the previous one, and we want to maintain the original template caching time
            return Ok(block_template);
        }

        // Rust rewrite:
        // We avoid passing a mempool ref to blockTemplateBuilder by calling
        // mempool.BlockCandidateTransactions and mempool.RemoveTransactions here.
        // We remove recursion seen in blockTemplateBuilder.BuildBlockTemplate here.
        loop {
            let transactions = self.block_candidate_transactions();
            match self.block_template_builder.build_block_template(consensus, miner_data, transactions) {
                Ok(block_template) => {
                    let block_template = cache_lock.set_immutable_cached_template(block_template);
                    return Ok(block_template.as_ref().clone());
                }
                Err(BuilderError::ConsensusError(BlockRuleError::InvalidTransactionsInNewBlock(invalid_transactions))) => {
                    let mut mempool_write = self.mempool.write();
                    invalid_transactions.iter().for_each(|(x, _)| {
                        let removal_result = mempool_write.remove_transaction(x, true);
                        if let Err(err) = removal_result {
                            // Original golang comment:
                            // mempool.remove_transactions might return errors in situations that are perfectly fine in this context.
                            // TODO: Once the mempool invariants are clear, this might return an error:
                            // https://github.com/kaspanet/kaspad/issues/1553
                            // NOTE: unlike golang, here we continue removing also if an error was found
                            error!("Error from mempool.remove_transactions: {:?}", err);
                        }
                    });
                }
                Err(err) => {
                    return Err(err)?;
                }
            }
        }
    }

    pub(crate) fn block_candidate_transactions(&self) -> Vec<CandidateTransaction> {
        self.mempool.read().block_candidate_transactions()
    }

    /// Clears the block template cache, forcing the next call to get_block_template to build a new block template.
    pub fn clear_block_template(&self) {
        self.block_template_cache.lock().clear();
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
        consensus: &dyn ConsensusApi,
        transaction: Transaction,
        priority: Priority,
        orphan: Orphan,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        self.validate_and_insert_mutable_transaction(consensus, MutableTransaction::from_tx(transaction), priority, orphan)
    }

    /// Exposed only for tests. Ordinary users should call `validate_and_insert_transaction` instead
    pub fn validate_and_insert_mutable_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: MutableTransaction,
        priority: Priority,
        orphan: Orphan,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        // read lock on mempool
        let mut transaction = self.mempool.read().pre_validate_and_populate_transaction(consensus, transaction)?;
        // no lock on mempool
        let validation_result = validate_mempool_transaction_and_populate(consensus, &mut transaction);
        // write lock on mempool
        let mut mempool = self.mempool.write();
        if let Some(accepted_transaction) =
            mempool.post_validate_and_insert_transaction(consensus, validation_result, transaction, priority, orphan)?
        {
            let unorphaned_transactions = mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction);
            drop(mempool);

            // The capacity used here may be exceeded since accepted unorphaned transaction may themselves unorphan other transactions.
            let mut accepted_transactions = Vec::with_capacity(unorphaned_transactions.len() + 1);
            // We include the original accepted transaction as well
            accepted_transactions.push(accepted_transaction);
            accepted_transactions.extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions));

            Ok(accepted_transactions)
        } else {
            Ok(vec![])
        }
    }

    fn validate_and_insert_unorphaned_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        mut incoming_transactions: Vec<MempoolTransaction>,
    ) -> Vec<Arc<Transaction>> {
        // The capacity used here may be exceeded (see next comment).
        let mut accepted_transactions = Vec::with_capacity(incoming_transactions.len());
        // We loop as long as incoming unorphaned transactions do unorphan other transactions when they
        // get validated and inserted into the mempool.
        while !incoming_transactions.is_empty() {
            // Since the consensus validation requires a slice of MutableTransaction, we destructure the vector of
            // MempoolTransaction into 2 distinct vectors holding respectively the needed MutableTransaction and Priority.
            let (mut transactions, priorities): (Vec<MutableTransaction>, Vec<Priority>) =
                incoming_transactions.into_iter().map(|x| (x.mtx, x.priority)).unzip();

            // no lock on mempool
            // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
            let mut lower_bound: usize = 0;
            let mut validation_results = Vec::with_capacity(transactions.len());
            while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
                validation_results
                    .extend(validate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
                lower_bound = upper_bound;
            }
            assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

            // write lock on mempool
            let mut mempool = self.mempool.write();
            incoming_transactions = transactions
                .into_iter()
                .zip(priorities)
                .zip(validation_results)
                .flat_map(|((transaction, priority), validation_result)| {
                    let orphan_id = transaction.id();
                    match mempool.post_validate_and_insert_transaction(
                        consensus,
                        validation_result,
                        transaction,
                        priority,
                        Orphan::Forbidden,
                    ) {
                        Ok(Some(accepted_transaction)) => {
                            accepted_transactions.push(accepted_transaction.clone());
                            mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction)
                        }
                        Ok(None) => vec![],
                        Err(err) => {
                            debug!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err);
                            vec![]
                        }
                    }
                })
                .collect::<Vec<_>>();
            drop(mempool);
        }
        accepted_transactions
    }

    /// Validates a batch of transactions, handling iteratively only the independent ones, and
    /// adds those to the set of known transactions that have not yet been added to any block.
    ///
    /// Returns transactions that where unorphaned following the insertion of the provided
    /// transactions. The returned transactions are clones of objects owned by the mempool.
    pub fn validate_and_insert_transaction_batch(
        &self,
        consensus: &dyn ConsensusApi,
        transactions: Vec<Transaction>,
        priority: Priority,
        orphan: Orphan,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        // The capacity used here may be exceeded since accepted transactions may unorphan other transactions.
        let mut accepted_transactions: Vec<Arc<Transaction>> = Vec::with_capacity(transactions.len());
        let mut batch = TransactionsStagger::new(transactions);
        while let Some(transactions) = batch.stagger() {
            if transactions.is_empty() {
                panic!(
                    "The mempool got a batch of transactions for validation with cyclic dependencies: {:?}",
                    transactions.iter().map(|x| x.id()).collect::<Vec<_>>()
                );
            }
            let mut transactions = transactions.into_iter().map(MutableTransaction::from_tx).collect::<Vec<_>>();

            // read lock on mempool
            // Here, we simply log and drop all erroneous transactions since the caller doesn't care about those anyway
            let mempool = self.mempool.read();
            transactions = transactions
                .into_iter()
                .filter_map(|tx| {
                    let transaction_id = tx.id();
                    match mempool.pre_validate_and_populate_transaction(consensus, tx) {
                        Ok(tx) => Some(tx),
                        Err(err) => {
                            debug!("Failed to pre validate transaction {0} due to rule error: {1}", transaction_id, err);
                            None
                        }
                    }
                })
                .collect();
            drop(mempool);

            // no lock on mempool
            // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
            let mut lower_bound: usize = 0;
            let mut validation_results = Vec::with_capacity(transactions.len());
            while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
                validation_results
                    .extend(validate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
                lower_bound = upper_bound;
            }
            assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

            // write lock on mempool
            // Here again, transactions failing post validation are logged and dropped
            let mut mempool = self.mempool.write();
            let unorphaned_transactions = transactions
                .into_iter()
                .zip(validation_results)
                .flat_map(|(transaction, validation_result)| {
                    let transaction_id = transaction.id();
                    match mempool.post_validate_and_insert_transaction(consensus, validation_result, transaction, priority, orphan) {
                        Ok(Some(accepted_transaction)) => {
                            accepted_transactions.push(accepted_transaction.clone());
                            mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction)
                        }
                        Ok(None) => {
                            // Either orphaned or already existing in the mempool
                            vec![]
                        }
                        Err(err) => {
                            debug!("Failed to post validate transaction {0} due to rule error: {1}", transaction_id, err);
                            vec![]
                        }
                    }
                })
                .collect::<Vec<_>>();
            drop(mempool);

            // TODO: handle RuleError::RejectInvalid errors when a banning process gets implemented
            accepted_transactions.extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions));
        }

        // Please note: the only reason this function returns a Result is the future handling of misbehaving nodes
        // and the related RuleError::RejectInvalid
        Ok(accepted_transactions)
    }

    fn next_transaction_chunk_upper_bound(&self, transactions: &[MutableTransaction], lower_bound: usize) -> Option<usize> {
        if lower_bound >= transactions.len() {
            return None;
        }
        let mut mass = 0;
        transactions[lower_bound..]
            .iter()
            .position(|tx| {
                mass += tx.calculated_mass.unwrap();
                mass >= self.block_template_builder.max_block_mass()
            })
            // Make sure the upper bound is greater than the lower bound, allowing to handle a very unlikely,
            // (if not impossible) case where the mass of a single transaction is greater than the maximum
            // chunk mass.
            .map(|relative_index| relative_index.max(1) + lower_bound)
            .or(Some(transactions.len()))
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

    /// Returns whether the mempool holds this transaction in any form.
    pub fn has_transaction(&self, transaction_id: &TransactionId, include_transaction_pool: bool, include_orphan_pool: bool) -> bool {
        self.mempool.read().has_transaction(transaction_id, include_transaction_pool, include_orphan_pool)
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
    ) -> GroupedOwnerTransactions {
        self.mempool.read().get_transactions_by_addresses(script_public_keys, include_transaction_pool, include_orphan_pool)
    }

    pub fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        self.mempool.read().transaction_count(include_transaction_pool, include_orphan_pool)
    }

    pub fn handle_new_block_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        block_transactions: &[Transaction],
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        // TODO: should use tx acceptance data to verify that new block txs are actually accepted into virtual state.

        // write lock on mempool
        let unorphaned_transactions = self.mempool.write().handle_new_block_transactions(block_transactions)?;

        // alternate no & write lock on mempool
        let accepted_transactions = self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions);

        // write lock on mempool
        self.mempool.write().expire_low_priority_transactions(consensus)?;

        Ok(accepted_transactions)
    }

    pub fn revalidate_high_priority_transactions(&self, consensus: &dyn ConsensusApi) -> MiningManagerResult<Vec<TransactionId>> {
        // read lock on mempool
        // Prepare a vector with clones of high priority transactions found in the mempool
        let transactions = self.mempool.read().all_transactions_with_priority(Priority::High);

        let mut valid_ids = Vec::with_capacity(transactions.len());

        // We process the transactions by level of dependency inside the batch.
        // Doing so allows to remove all chained dependencies of rejected transactions before actually trying
        // to revalidate those, saving potentially a lot of computing resources.
        let mut batch = TransactionsStagger::new(transactions);
        while let Some(transactions) = batch.stagger() {
            if transactions.is_empty() {
                panic!(
                    "The mempool high priorities transactions have cyclic dependencies: {:?}",
                    transactions.iter().map(|x| x.id()).collect::<Vec<_>>()
                );
            }

            // read lock on mempool
            // As the revalidation process is no longer atomic, we filter the transactions ready for revalidation,
            // keeping only the ones actually present in the mempool (see comment above).
            let mempool = self.mempool.read();
            let mut transactions = transactions
                .into_iter()
                .filter_map(|mut x| {
                    if mempool.has_transaction(&x.id(), true, false) {
                        x.clear_entries();
                        mempool.populate_mempool_entries(&mut x);
                        Some(x)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            drop(mempool);

            // no lock on mempool
            // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
            let mut lower_bound: usize = 0;
            let mut validation_results = Vec::with_capacity(transactions.len());
            while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
                validation_results
                    .extend(validate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
                lower_bound = upper_bound;
            }
            assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

            // write lock on mempool
            // Depending on the validation result, transactions are either accepted or removed
            let mut mempool = self.mempool.write();
            for (transaction, validation_result) in transactions.into_iter().zip(validation_results) {
                let transaction_id = transaction.id();
                // Only consider transactions still being in the mempool since during the validation some might have been removed.
                if mempool.update_revalidated_transaction(transaction) {
                    match validation_result {
                        Ok(()) => {
                            // A following transaction should not remove this one from the pool since we process in a topological order.
                            // Still, considering the (very unlikely) scenario of two high priority txs sandwiching a low one, where
                            // in this case topological order is not guaranteed since we only considered chained dependencies of
                            // high-priority transactions, we might wrongfully return as valid the id of a removed transaction.
                            // However, as only consequence, said transaction would then be advertised to registered peers and not be
                            // provided upon request.
                            valid_ids.push(transaction_id);
                        }
                        Err(RuleError::RejectMissingOutpoint) => {
                            warn!(
                                "Removing high priority transaction {0} and its redeemers for missing outpoint during revalidation",
                                transaction_id
                            );
                            // This call cleanly removes the invalid transaction and its redeemers.
                            mempool.remove_transaction(&transaction_id, true)?;
                        }
                        Err(err) => {
                            // Rust rewrite note:
                            // The behavior changes here compared to the golang version.
                            // The failed revalidation is simply logged and the process continues.
                            warn!(
                                "Removing high priority  transaction {0} and its redeemers, it failed revalidation with {1}",
                                transaction_id, err
                            );
                            // This call cleanly removes the invalid transaction and its redeemers.
                            mempool.remove_transaction(&transaction_id, true)?;
                        }
                    }
                }
            }
            drop(mempool);
        }
        // Return the successfully processed high priority transaction ids
        Ok(valid_ids)
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

/// Async proxy for the mining manager
#[derive(Clone)]
pub struct MiningManagerProxy {
    inner: Arc<MiningManager>,
}

impl MiningManagerProxy {
    pub fn new(inner: Arc<MiningManager>) -> Self {
        Self { inner }
    }

    pub async fn get_block_template(self, consensus: &ConsensusProxy, miner_data: MinerData) -> MiningManagerResult<BlockTemplate> {
        consensus.clone().spawn_blocking(move |c| self.inner.get_block_template(c, &miner_data)).await
    }

    /// Clears the block template cache, forcing the next call to get_block_template to build a new block template.
    pub fn clear_block_template(&self) {
        self.inner.clear_block_template()
    }

    /// Validates a transaction and adds it to the set of known transactions that have not yet been
    /// added to any block.
    ///
    /// The returned transactions are clones of objects owned by the mempool.
    pub async fn validate_and_insert_transaction(
        self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
        priority: Priority,
        orphan: Orphan,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        consensus.clone().spawn_blocking(move |c| self.inner.validate_and_insert_transaction(c, transaction, priority, orphan)).await
    }

    /// Validates a batch of transactions, handling iteratively only the independent ones, and
    /// adds those to the set of known transactions that have not yet been added to any block.
    ///
    /// Returns transactions that where unorphaned following the insertion of the provided
    /// transactions. The returned transactions are clones of objects owned by the mempool.
    pub async fn validate_and_insert_transaction_batch(
        self,
        consensus: &ConsensusProxy,
        transactions: Vec<Transaction>,
        priority: Priority,
        orphan: Orphan,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.validate_and_insert_transaction_batch(c, transactions, priority, orphan))
            .await
    }

    pub async fn handle_new_block_transactions(
        self,
        consensus: &ConsensusProxy,
        block_transactions: Arc<Vec<Transaction>>,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        consensus.clone().spawn_blocking(move |c| self.inner.handle_new_block_transactions(c, &block_transactions)).await
    }

    pub async fn revalidate_high_priority_transactions(self, consensus: &ConsensusProxy) -> MiningManagerResult<Vec<TransactionId>> {
        consensus.clone().spawn_blocking(move |c| self.inner.revalidate_high_priority_transactions(c)).await
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transaction(
        self,
        transaction_id: TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> Option<MutableTransaction> {
        spawn_blocking(move || self.inner.get_transaction(&transaction_id, include_transaction_pool, include_orphan_pool))
            .await
            .unwrap()
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub async fn has_transaction(
        self,
        transaction_id: TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> bool {
        spawn_blocking(move || self.inner.has_transaction(&transaction_id, include_transaction_pool, include_orphan_pool))
            .await
            .unwrap()
    }

    pub async fn transaction_count(self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        spawn_blocking(move || self.inner.transaction_count(include_transaction_pool, include_orphan_pool)).await.unwrap()
    }

    pub async fn get_all_transactions(
        self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        spawn_blocking(move || self.inner.get_all_transactions(include_transaction_pool, include_orphan_pool)).await.unwrap()
    }

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transactions_by_addresses(
        self,
        script_public_keys: ScriptPublicKeySet,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> GroupedOwnerTransactions {
        spawn_blocking(move || {
            self.inner.get_transactions_by_addresses(&script_public_keys, include_transaction_pool, include_orphan_pool)
        })
        .await
        .unwrap()
    }
}
