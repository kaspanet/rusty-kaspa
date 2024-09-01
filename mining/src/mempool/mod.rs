use crate::{
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    model::{
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
        tx_query::TransactionQuery,
    },
    MiningCounters,
};

use self::{
    config::Config,
    model::{accepted_transactions::AcceptedTransactions, orphan_pool::OrphanPool, pool::Pool, transactions_pool::TransactionsPool},
    tx::Priority,
};
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{MutableTransaction, TransactionId},
};
use kaspa_core::time::Stopwatch;
use std::sync::Arc;

pub(crate) mod check_transaction_standard;
pub mod config;
pub mod errors;
pub(crate) mod handle_new_block_transactions;
pub(crate) mod model;
pub(crate) mod populate_entries_and_try_validate;
pub(crate) mod remove_transaction;
pub(crate) mod replace_by_fee;
pub(crate) mod validate_and_insert_transaction;

/// Mempool contains transactions intended to be inserted into a block and mined.
///
/// Some important properties to consider:
///
/// - Transactions can be chained, so a transaction can have parents and chained
///   dependencies in the mempool.
/// - A transaction can have some of its outpoints refer to missing outputs when
///   added to the mempool. In this case it is considered orphan.
/// - An orphan transaction is unorphaned when all its UTXO entries have been
///   built or found.
/// - There are transaction priorities: high and low.
/// - Transactions submitted to the mempool by a RPC call have **high priority**.
///   They are owned by the node, they never expire in the mempool and the node
///   rebroadcasts them once in a while.
/// - Transactions received through P2P have **low-priority**. They expire after
///   60 seconds and are removed if not inserted in a block for mining.
pub(crate) struct Mempool {
    config: Arc<Config>,
    transaction_pool: TransactionsPool,
    orphan_pool: OrphanPool,
    accepted_transactions: AcceptedTransactions,
    counters: Arc<MiningCounters>,
}

impl Mempool {
    pub(crate) fn new(config: Arc<Config>, counters: Arc<MiningCounters>) -> Self {
        let transaction_pool = TransactionsPool::new(config.clone());
        let orphan_pool = OrphanPool::new(config.clone());
        let accepted_transactions = AcceptedTransactions::new(config.clone());
        Self { config, transaction_pool, orphan_pool, accepted_transactions, counters }
    }

    pub(crate) fn get_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> Option<MutableTransaction> {
        let mut transaction = None;
        if query.include_transaction_pool() {
            transaction = self.transaction_pool.get(transaction_id);
        }
        if transaction.is_none() && query.include_orphan_pool() {
            transaction = self.orphan_pool.get(transaction_id);
        }
        transaction.map(|x| x.mtx.clone())
    }

    pub(crate) fn has_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> bool {
        (query.include_transaction_pool() && self.transaction_pool.has(transaction_id))
            || (query.include_orphan_pool() && self.orphan_pool.has(transaction_id))
    }

    pub(crate) fn get_all_transactions(&self, query: TransactionQuery) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        let transactions = if query.include_transaction_pool() { self.transaction_pool.get_all_transactions() } else { vec![] };
        let orphans = if query.include_orphan_pool() { self.orphan_pool.get_all_transactions() } else { vec![] };
        (transactions, orphans)
    }

    pub(crate) fn get_all_transaction_ids(&self, query: TransactionQuery) -> (Vec<TransactionId>, Vec<TransactionId>) {
        let transactions = if query.include_transaction_pool() { self.transaction_pool.get_all_transaction_ids() } else { vec![] };
        let orphans = if query.include_orphan_pool() { self.orphan_pool.get_all_transaction_ids() } else { vec![] };
        (transactions, orphans)
    }

    pub(crate) fn get_transactions_by_addresses(
        &self,
        script_public_keys: &ScriptPublicKeySet,
        query: TransactionQuery,
    ) -> GroupedOwnerTransactions {
        let mut owner_set = GroupedOwnerTransactions::default();
        if query.include_transaction_pool() {
            self.transaction_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        if query.include_orphan_pool() {
            self.orphan_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        owner_set
    }

    pub(crate) fn transaction_count(&self, query: TransactionQuery) -> usize {
        let mut count = 0;
        if query.include_transaction_pool() {
            count += self.transaction_pool.len()
        }
        if query.include_orphan_pool() {
            count += self.orphan_pool.len()
        }
        count
    }

    pub(crate) fn ready_transaction_count(&self) -> usize {
        self.transaction_pool.ready_transaction_count()
    }

    pub(crate) fn ready_transaction_total_mass(&self) -> u64 {
        self.transaction_pool.ready_transaction_total_mass()
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier
    pub(crate) fn build_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        let _sw = Stopwatch::<10>::with_threshold("build_selector op");
        self.transaction_pool.build_selector()
    }

    /// Builds a feerate estimator based on internal state of the ready transactions frontier
    pub(crate) fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        self.transaction_pool.build_feerate_estimator(args)
    }

    pub(crate) fn all_transaction_ids_with_priority(&self, priority: Priority) -> Vec<TransactionId> {
        let _sw = Stopwatch::<15>::with_threshold("all_transaction_ids_with_priority op");
        self.transaction_pool.all_transaction_ids_with_priority(priority)
    }

    pub(crate) fn update_revalidated_transaction(&mut self, transaction: MutableTransaction) -> bool {
        self.transaction_pool.update_revalidated_transaction(transaction)
    }

    pub(crate) fn has_accepted_transaction(&self, transaction_id: &TransactionId) -> bool {
        self.accepted_transactions.has(transaction_id)
    }

    pub(crate) fn unaccepted_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.accepted_transactions.unaccepted(&mut transactions.into_iter())
    }

    pub(crate) fn unknown_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        let mut not_in_pools_txs = transactions
            .into_iter()
            .filter(|transaction_id| !(self.transaction_pool.has(transaction_id) || self.orphan_pool.has(transaction_id)));
        self.accepted_transactions.unaccepted(&mut not_in_pools_txs)
    }

    #[cfg(test)]
    pub(crate) fn get_estimated_size(&self) -> usize {
        self.transaction_pool.get_estimated_size()
    }
}

pub mod tx {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Priority {
        Low,
        High,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Orphan {
        Forbidden,
        Allowed,
    }

    /// Replace by Fee (RBF) policy
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RbfPolicy {
        /// ### RBF is forbidden
        ///
        /// Inserts the incoming transaction.
        ///
        /// Conditions of success:
        ///
        /// - no double spend
        ///
        /// If conditions are not met, leaves the mempool unchanged and fails with a double spend error.
        Forbidden,

        /// ### RBF may occur
        ///
        /// Identifies double spends in mempool and their owning transactions checking in order every input of the incoming
        /// transaction.
        ///
        /// Removes all mempool transactions owning double spends and inserts the incoming transaction.
        ///
        /// Conditions of success:
        ///
        /// - on absence of double spends, always succeeds
        /// - on double spends, the incoming transaction has a higher fee/mass ratio than the mempool transaction owning
        ///   the first double spend
        ///
        /// If conditions are not met, leaves the mempool unchanged and fails with a double spend or a tx fee/mass too low error.
        Allowed,

        /// ### RBF must occur
        ///
        /// Identifies double spends in mempool and their owning transactions checking in order every input of the incoming
        /// transaction.
        ///
        /// Removes the mempool transaction owning the double spends and inserts the incoming transaction.
        ///
        /// Conditions of success:
        ///
        /// - at least one double spend
        /// - all double spends belong to the same mempool transaction
        /// - the incoming transaction has a higher fee/mass ratio than the mempool double spending transaction.
        ///
        /// If conditions are not met, leaves the mempool unchanged and fails with a double spend or a tx fee/mass too low error.
        Mandatory,
    }
}
