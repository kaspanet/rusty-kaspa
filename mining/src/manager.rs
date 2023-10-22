use crate::{
    block_template::{builder::BlockTemplateBuilder, errors::BuilderError},
    cache::BlockTemplateCache,
    errors::MiningManagerResult,
    mempool::{
        config::Config,
        model::tx::{MempoolTransaction, TxRemovalReason},
        populate_entries_and_try_validate::{
            populate_mempool_transactions_in_parallel, validate_mempool_transaction, validate_mempool_transactions_in_parallel,
        },
        tx::{Orphan, Priority},
        Mempool,
    },
    model::{
        candidate_tx::CandidateTransaction,
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
        topological_sort::IntoIterTopologically,
    },
    MiningCounters,
};
use itertools::Itertools;
use kaspa_consensus_core::{
    api::ConsensusApi,
    block::{BlockTemplate, TemplateBuildMode},
    coinbase::MinerData,
    errors::{block::RuleError as BlockRuleError, tx::TxRuleError},
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutput},
};
use kaspa_consensusmanager::{spawn_blocking, ConsensusProxy};
use kaspa_core::{debug, error, info, time::Stopwatch, warn};
use kaspa_mining_errors::{manager::MiningManagerError, mempool::RuleError};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub struct MiningManager {
    config: Arc<Config>,
    block_template_cache: BlockTemplateCache,
    mempool: RwLock<Mempool>,
    counters: Arc<MiningCounters>,
}

impl MiningManager {
    pub fn new(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
    ) -> Self {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(config, cache_lifetime, counters)
    }

    pub fn new_with_spam_blocking_option(
        block_spam_txs: bool,
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
    ) -> Self {
        let config = Config::build_default_with_spam_blocking_option(
            block_spam_txs,
            target_time_per_block,
            relay_non_std_transactions,
            max_block_mass,
        );
        Self::with_config(config, cache_lifetime, counters)
    }

    pub(crate) fn with_config(config: Config, cache_lifetime: Option<u64>, counters: Arc<MiningCounters>) -> Self {
        let config = Arc::new(config);
        let mempool = RwLock::new(Mempool::new(config.clone(), counters.clone()));
        let block_template_cache = BlockTemplateCache::new(cache_lifetime);
        Self { config, block_template_cache, mempool, counters }
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
            let block_template = BlockTemplateBuilder::modify_block_template(consensus, miner_data, &immutable_template)?;

            // No point in updating cache since we have no reason to believe this coinbase will be used more
            // than the previous one, and we want to maintain the original template caching time
            return Ok(block_template);
        }

        // Rust rewrite:
        // We avoid passing a mempool ref to blockTemplateBuilder by calling
        // mempool.BlockCandidateTransactions and mempool.RemoveTransactions here.
        // We remove recursion seen in blockTemplateBuilder.BuildBlockTemplate here.
        debug!("Building a new block template...");
        let _swo = Stopwatch::<22>::with_threshold("build_block_template full loop");
        let mut attempts: u64 = 0;
        loop {
            attempts += 1;

            let transactions = self.block_candidate_transactions();
            let block_template_builder = BlockTemplateBuilder::new(self.config.maximum_mass_per_block);
            let build_mode = if attempts < self.config.maximum_build_block_template_attempts {
                TemplateBuildMode::Standard
            } else {
                TemplateBuildMode::Infallible
            };
            match block_template_builder.build_block_template(consensus, miner_data, transactions, build_mode) {
                Ok(block_template) => {
                    let block_template = cache_lock.set_immutable_cached_template(block_template);
                    match attempts {
                        1 => {
                            debug!(
                                "Built a new block template with {} transactions in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        2 => {
                            debug!(
                                "Built a new block template with {} transactions at second attempt in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        n => {
                            debug!(
                                "Built a new block template with {} transactions in {} attempts totaling {:#?}",
                                block_template.block.transactions.len(),
                                n,
                                _swo.elapsed()
                            );
                        }
                    }
                    return Ok(block_template.as_ref().clone());
                }
                Err(BuilderError::ConsensusError(BlockRuleError::InvalidTransactionsInNewBlock(invalid_transactions))) => {
                    let mut missing_outpoint: usize = 0;
                    let mut invalid: usize = 0;

                    let mut mempool_write = self.mempool.write();
                    invalid_transactions.iter().for_each(|(x, err)| {
                        // On missing outpoints, the most likely is that the tx was already in a block accepted by
                        // the consensus but not yet processed by handle_new_block_transactions(). Another possibility
                        // is a double spend. In both cases, we simply remove the transaction but keep its redeemers.
                        // Those will either be valid in a next block template or invalidated if it's a double spend.
                        //
                        // If the redeemers of a transaction accepted in consensus but not yet handled in mempool were
                        // removed, it would lead to having subsequently submitted children transactions of the removed
                        // redeemers being unexpectedly either orphaned or rejected in case orphans are disallowed.
                        //
                        // For all other errors, we do remove the redeemers.

                        let removal_result = if *err == TxRuleError::MissingTxOutpoints {
                            missing_outpoint += 1;
                            mempool_write.remove_transaction(x, false, TxRemovalReason::Muted, "")
                        } else {
                            invalid += 1;
                            warn!("Remove per BBT invalid transaction and descendants");
                            mempool_write.remove_transaction(
                                x,
                                true,
                                TxRemovalReason::InvalidInBlockTemplate,
                                format!(" error: {}", err).as_str(),
                            )
                        };
                        if let Err(err) = removal_result {
                            // Original golang comment:
                            // mempool.remove_transactions might return errors in situations that are perfectly fine in this context.
                            // TODO: Once the mempool invariants are clear, this might return an error:
                            // https://github.com/kaspanet/kaspad/issues/1553
                            // NOTE: unlike golang, here we continue removing also if an error was found
                            error!("Error from mempool.remove_transactions: {:?}", err);
                        }
                    });
                    drop(mempool_write);

                    debug!(
                        "Building a new block template failed for {} txs missing outpoint and {} invalid txs",
                        missing_outpoint, invalid
                    );
                }
                Err(err) => {
                    warn!("Building a new block template failed: {}", err);
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
        self.block_template_cache.clear();
    }

    #[cfg(test)]
    pub(crate) fn block_template_builder(&self) -> BlockTemplateBuilder {
        BlockTemplateBuilder::new(self.config.maximum_mass_per_block)
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
        let validation_result = validate_mempool_transaction(consensus, &mut transaction);
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
            self.counters.increase_tx_counts(1, priority);

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
                assert!(lower_bound < upper_bound, "the chunk is never empty");
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
                            self.counters.increase_tx_counts(1, priority);
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
    ) -> Vec<MiningManagerResult<Arc<Transaction>>> {
        const TRANSACTION_CHUNK_SIZE: usize = 250;

        // The capacity used here may be exceeded since accepted transactions may unorphan other transactions.
        let mut insert_results: Vec<MiningManagerResult<Arc<Transaction>>> = Vec::with_capacity(transactions.len());
        let mut unorphaned_transactions = vec![];
        let _swo = Stopwatch::<80>::with_threshold("validate_and_insert_transaction_batch topological_sort op");
        let sorted_transactions = transactions.into_iter().map(MutableTransaction::from_tx).topological_into_iter();
        drop(_swo);

        // read lock on mempool
        // Here, we simply log and drop all erroneous transactions since the caller doesn't care about those anyway
        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            let txs = chunk.filter_map(|tx| {
                let transaction_id = tx.id();
                match mempool.pre_validate_and_populate_transaction(consensus, tx) {
                    Ok(tx) => Some(tx),
                    Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                        debug!("Ignoring already accepted transaction {}", transaction_id);
                        None
                    }
                    Err(RuleError::RejectDuplicate(transaction_id)) => {
                        debug!("Ignoring transaction already in the mempool {}", transaction_id);
                        None
                    }
                    Err(RuleError::RejectDuplicateOrphan(transaction_id)) => {
                        debug!("Ignoring transaction already in the orphan pool {}", transaction_id);
                        None
                    }
                    Err(err) => {
                        debug!("Failed to pre validate transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                        None
                    }
                }
            });
            transactions.extend(txs);
        }

        // no lock on mempool
        // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            validation_results
                .extend(validate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        // write lock on mempool
        // Here again, transactions failing post validation are logged and dropped
        for chunk in &transactions.into_iter().zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut mempool = self.mempool.write();
            let txs = chunk.flat_map(|(transaction, validation_result)| {
                let transaction_id = transaction.id();
                match mempool.post_validate_and_insert_transaction(consensus, validation_result, transaction, priority, orphan) {
                    Ok(Some(accepted_transaction)) => {
                        insert_results.push(Ok(accepted_transaction.clone()));
                        self.counters.increase_tx_counts(1, priority);
                        mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction)
                    }
                    Ok(None) => {
                        // Either orphaned or already existing in the mempool
                        vec![]
                    }
                    Err(err) => {
                        debug!("Failed to post validate transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                        vec![]
                    }
                }
            });
            unorphaned_transactions.extend(txs);
        }

        insert_results
            .extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions).into_iter().map(Ok));
        insert_results
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
                mass >= self.config.maximum_mass_per_block
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
        assert!(include_transaction_pool || include_orphan_pool, "at least one of either transactions or orphans must be included");
        self.mempool.read().get_transaction(transaction_id, include_transaction_pool, include_orphan_pool)
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub fn has_transaction(&self, transaction_id: &TransactionId, include_transaction_pool: bool, include_orphan_pool: bool) -> bool {
        assert!(include_transaction_pool || include_orphan_pool, "at least one of either transactions or orphans must be included");
        self.mempool.read().has_transaction(transaction_id, include_transaction_pool, include_orphan_pool)
    }

    pub fn get_all_transactions(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;
        assert!(include_transaction_pool || include_orphan_pool, "at least one of either transactions or orphans must be included");
        // read lock on mempool by transaction chunks
        let transactions = if include_transaction_pool {
            let transaction_ids = self.mempool.read().get_all_transaction_ids(true, false).0;
            let mut transactions = Vec::with_capacity(self.mempool.read().transaction_count(true, false));
            for chunks in transaction_ids.chunks(TRANSACTION_CHUNK_SIZE) {
                let mempool = self.mempool.read();
                transactions.extend(chunks.iter().filter_map(|x| mempool.get_transaction(x, true, false)));
            }
            transactions
        } else {
            vec![]
        };
        // read lock on mempool
        let orphans = if include_orphan_pool { self.mempool.read().get_all_transactions(false, true).1 } else { vec![] };
        (transactions, orphans)
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
        assert!(include_transaction_pool || include_orphan_pool, "at least one of either transactions or orphans must be included");
        // TODO: break the monolithic lock
        self.mempool.read().get_transactions_by_addresses(script_public_keys, include_transaction_pool, include_orphan_pool)
    }

    pub fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        assert!(include_transaction_pool || include_orphan_pool, "at least one of either transactions or orphans must be included");
        self.mempool.read().transaction_count(include_transaction_pool, include_orphan_pool)
    }

    pub fn handle_new_block_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        block_daa_score: u64,
        block_transactions: &[Transaction],
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        // TODO: should use tx acceptance data to verify that new block txs are actually accepted into virtual state.
        // TODO: avoid returning a result from this function (and the underlying function). Any possible error is a
        // problem of the internal implementation and unrelated to the caller

        // write lock on mempool
        let unorphaned_transactions = self.mempool.write().handle_new_block_transactions(block_daa_score, block_transactions)?;

        // alternate no & write lock on mempool
        let accepted_transactions = self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions);

        Ok(accepted_transactions)
    }

    pub fn expire_low_priority_transactions(&self, consensus: &dyn ConsensusApi) {
        // very fine-grained write locks on mempool
        debug!("<> Expiring low priority transactions...");

        // orphan pool
        if let Err(err) = self.mempool.write().expire_orphan_low_priority_transactions(consensus) {
            warn!("Failed to expire transactions from orphan pool: {}", err);
        }

        // accepted transaction cache
        self.mempool.write().expire_accepted_transactions(consensus);

        // mempool
        let expired_low_priority_transactions = self.mempool.write().collect_expired_low_priority_transactions(consensus);
        for chunk in &expired_low_priority_transactions.iter().chunks(24) {
            let mut mempool = self.mempool.write();
            chunk.into_iter().for_each(|tx| {
                if let Err(err) = mempool.remove_transaction(tx, true, TxRemovalReason::Muted, "") {
                    warn!("Failed to remove transaction {} from mempool: {}", tx, err);
                }
            });
        }
        match expired_low_priority_transactions.len() {
            0 => {}
            1 => debug!("Removed transaction ({}) {}", TxRemovalReason::Expired, expired_low_priority_transactions[0]),
            n => debug!("Removed {} transactions ({}): {}...", n, TxRemovalReason::Expired, expired_low_priority_transactions[0]),
        }
    }

    pub fn revalidate_high_priority_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;

        // read lock on mempool
        // Prepare a vector with clones of high priority transactions found in the mempool
        let mempool = self.mempool.read();
        let transaction_ids = mempool.all_transaction_ids_with_priority(Priority::High);
        if transaction_ids.is_empty() {
            debug!("<> Revalidating high priority transactions found no transactions");
            return;
        } else {
            debug!("<> Revalidating {} high priority transactions...", transaction_ids.len());
        }
        drop(mempool);
        // read lock on mempool by transaction chunks
        let mut transactions = Vec::with_capacity(transaction_ids.len());
        for chunk in &transaction_ids.iter().chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            transactions.extend(chunk.filter_map(|x| mempool.get_transaction(x, true, false)));
        }

        let mut valid: usize = 0;
        let mut accepted: usize = 0;
        let mut other: usize = 0;
        let mut missing_outpoint: usize = 0;
        let mut invalid: usize = 0;

        // We process the transactions by level of dependency inside the batch.
        // Doing so allows to remove all chained dependencies of rejected transactions.
        let _swo = Stopwatch::<800>::with_threshold("revalidate topological_sort op");
        let sorted_transactions = transactions.topological_into_iter();
        drop(_swo);

        // read lock on mempool by transaction chunks
        // As the revalidation process is no longer atomic, we filter the transactions ready for revalidation,
        // keeping only the ones actually present in the mempool (see comment above).
        let _swo = Stopwatch::<900>::with_threshold("revalidate populate_mempool_entries op");
        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            let txs = chunk.filter_map(|mut x| {
                let transaction_id = x.id();
                if mempool.has_accepted_transaction(&transaction_id) {
                    accepted += 1;
                    None
                } else if mempool.has_transaction(&transaction_id, true, false) {
                    x.clear_entries();
                    mempool.populate_mempool_entries(&mut x);
                    match x.is_fully_populated() {
                        false => Some(x),
                        true => {
                            // If all entries are populated with mempool UTXOs, we already know the transaction is valid
                            valid += 1;
                            None
                        }
                    }
                } else {
                    other += 1;
                    None
                }
            });
            transactions.extend(txs);
        }
        drop(_swo);

        // no lock on mempool
        // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            let _swo = Stopwatch::<60>::with_threshold("revalidate validate_mempool_transactions_in_parallel op");
            validation_results
                .extend(populate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
            drop(_swo);
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        // write lock on mempool
        // Depending on the validation result, transactions are either accepted or removed
        for chunk in &transactions.into_iter().zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut valid_ids = Vec::with_capacity(TRANSACTION_CHUNK_SIZE);
            let mut mempool = self.mempool.write();
            let _swo = Stopwatch::<60>::with_threshold("revalidate update_revalidated_transaction op");
            for (transaction, validation_result) in chunk {
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
                            valid += 1;
                        }
                        Err(RuleError::RejectMissingOutpoint) => {
                            let transaction = mempool.get_transaction(&transaction_id, true, false).unwrap();
                            let missing_txs = transaction
                                .entries
                                .iter()
                                .zip(transaction.tx.inputs.iter())
                                .flat_map(
                                    |(entry, input)| {
                                        if entry.is_none() {
                                            Some(input.previous_outpoint.transaction_id)
                                        } else {
                                            None
                                        }
                                    },
                                )
                                .collect::<Vec<_>>();

                            // A transaction may have missing outpoints for legitimate reasons related to concurrency, like a race condition between
                            // an accepted block having not started yet or unfinished call to handle_new_block_transactions but already processed by
                            // the consensus and this ongoing call to revalidate.
                            //
                            // So we only remove the transaction and keep its redeemers in the mempool because we cannot be sure they are invalid, in
                            // fact in the race condition case they are valid regarding outpoints.
                            let extra_info = match missing_txs.len() {
                                0 => " but no missing tx!".to_string(), // this is never supposed to happen
                                1 => format!(" missing tx {}", missing_txs[0]),
                                n => format!(" with {} missing txs {}..{}", n, missing_txs[0], missing_txs.last().unwrap()),
                            };

                            // This call cleanly removes the invalid transaction.
                            let result = mempool.remove_transaction(
                                &transaction_id,
                                false,
                                TxRemovalReason::RevalidationWithMissingOutpoints,
                                extra_info.as_str(),
                            );
                            if let Err(err) = result {
                                warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err);
                            }
                            missing_outpoint += 1;
                        }
                        Err(err) => {
                            // Rust rewrite note:
                            // The behavior changes here compared to the golang version.
                            // The failed revalidation is simply logged and the process continues.
                            warn!(
                                "Removing high priority transaction {0} and its redeemers, it failed revalidation with {1}",
                                transaction_id, err
                            );
                            // This call cleanly removes the invalid transaction and its redeemers.
                            let result = mempool.remove_transaction(&transaction_id, true, TxRemovalReason::Muted, "");
                            if let Err(err) = result {
                                warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err);
                            }
                            invalid += 1;
                        }
                    }
                } else {
                    other += 1;
                }
            }
            if !valid_ids.is_empty() {
                let _ = transaction_ids_sender.send(valid_ids);
            }
            drop(_swo);
            drop(mempool);
        }
        match accepted + missing_outpoint + invalid {
            0 => {
                info!("Revalidated {} high priority transactions", valid);
            }
            _ => {
                info!(
                    "Revalidated {} and removed {} high priority transactions (removals: {} accepted, {} missing outpoint, {} invalid)",
                    valid,
                    accepted + missing_outpoint + invalid,
                    accepted,
                    missing_outpoint,
                    invalid,
                );
            }
        }
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

    pub fn has_accepted_transaction(&self, transaction_id: &TransactionId) -> bool {
        self.mempool.read().has_accepted_transaction(transaction_id)
    }

    pub fn unaccepted_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unaccepted_transactions(transactions)
    }

    pub fn unknown_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unknown_transactions(transactions)
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
    ) -> Vec<MiningManagerResult<Arc<Transaction>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.validate_and_insert_transaction_batch(c, transactions, priority, orphan))
            .await
    }

    pub async fn handle_new_block_transactions(
        self,
        consensus: &ConsensusProxy,
        block_daa_score: u64,
        block_transactions: Arc<Vec<Transaction>>,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.handle_new_block_transactions(c, block_daa_score, &block_transactions))
            .await
    }

    pub async fn expire_low_priority_transactions(self, consensus: &ConsensusProxy) {
        consensus.clone().spawn_blocking(move |c| self.inner.expire_low_priority_transactions(c)).await;
    }

    pub async fn revalidate_high_priority_transactions(
        self,
        consensus: &ConsensusProxy,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        consensus.clone().spawn_blocking(move |c| self.inner.revalidate_high_priority_transactions(c, transaction_ids_sender)).await;
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

    /// Returns whether a transaction id was registered as accepted in the mempool, meaning
    /// that the consensus accepted a block containing it and said block was handled by the
    /// mempool.
    ///
    /// Registered transaction ids expire after a delay and are unregistered from the mempool.
    /// So a returned value of true means with certitude that the transaction was accepted and
    /// a false means either the transaction was never accepted or it was but beyond the expiration
    /// delay.
    pub async fn has_accepted_transaction(self, transaction_id: TransactionId) -> bool {
        spawn_blocking(move || self.inner.has_accepted_transaction(&transaction_id)).await.unwrap()
    }

    /// Returns a vector of unaccepted transactions.
    /// For more details, see [`Self::has_accepted_transaction()`].
    pub async fn unaccepted_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unaccepted_transactions(transactions)).await.unwrap()
    }

    /// Returns a vector with all transaction ids that are neither in the mempool, nor in the orphan pool
    /// nor accepted.
    pub async fn unknown_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unknown_transactions(transactions)).await.unwrap()
    }
}
