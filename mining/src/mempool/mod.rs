use crate::{
    model::{
        candidate_tx::CandidateTransaction,
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
    },
    MiningCounters,
};

use self::{
    config::Config,
    model::{accepted_transactions::AcceptedTransactions, orphan_pool::OrphanPool, pool::Pool, transactions_pool::TransactionsPool},
    tx::Priority,
};
use kaspa_consensus_core::tx::{MutableTransaction, TransactionId};
use kaspa_core::time::Stopwatch;
use std::sync::Arc;

pub(crate) mod check_transaction_standard;
pub mod config;
pub mod errors;
pub(crate) mod handle_new_block_transactions;
pub(crate) mod model;
pub(crate) mod populate_entries_and_try_validate;
pub(crate) mod remove_transaction;
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

    pub(crate) fn get_transaction(
        &self,
        transaction_id: &TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> Option<MutableTransaction> {
        let mut transaction = None;
        if include_transaction_pool {
            transaction = self.transaction_pool.get(transaction_id);
        }
        if transaction.is_none() && include_orphan_pool {
            transaction = self.orphan_pool.get(transaction_id);
        }
        transaction.map(|x| x.mtx.clone())
    }

    pub(crate) fn has_transaction(
        &self,
        transaction_id: &TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> bool {
        (include_transaction_pool && self.transaction_pool.has(transaction_id))
            || (include_orphan_pool && self.orphan_pool.has(transaction_id))
    }

    pub(crate) fn get_all_transactions(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        let transactions = if include_transaction_pool { self.transaction_pool.get_all_transactions() } else { vec![] };
        let orphans = if include_orphan_pool { self.orphan_pool.get_all_transactions() } else { vec![] };
        (transactions, orphans)
    }

    pub(crate) fn get_all_transaction_ids(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<TransactionId>, Vec<TransactionId>) {
        let transactions = if include_transaction_pool { self.transaction_pool.get_all_transaction_ids() } else { vec![] };
        let orphans = if include_orphan_pool { self.orphan_pool.get_all_transaction_ids() } else { vec![] };
        (transactions, orphans)
    }

    pub(crate) fn get_transactions_by_addresses(
        &self,
        script_public_keys: &ScriptPublicKeySet,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> GroupedOwnerTransactions {
        let mut owner_set = GroupedOwnerTransactions::default();
        if include_transaction_pool {
            self.transaction_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        if include_orphan_pool {
            self.orphan_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        owner_set
    }

    pub(crate) fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        let mut count = 0;
        if include_transaction_pool {
            count += self.transaction_pool.len()
        }
        if include_orphan_pool {
            count += self.orphan_pool.len()
        }
        count
    }

    pub(crate) fn block_candidate_transactions(&self) -> Vec<CandidateTransaction> {
        let _sw = Stopwatch::<10>::with_threshold("block_candidate_transactions op");
        self.transaction_pool.all_ready_transactions()
    }

    pub(crate) fn all_transaction_ids_with_priority(&self, priority: Priority) -> Vec<TransactionId> {
        let _sw = Stopwatch::<15>::with_threshold("all_transaction_ids_with_priority op");
        self.transaction_pool.all_transaction_ids_with_priority(priority)
    }

    pub(crate) fn update_revalidated_transaction(&mut self, transaction: MutableTransaction) -> bool {
        if let Some(tx) = self.transaction_pool.get_mut(&transaction.id()) {
            tx.mtx = transaction;
            true
        } else {
            false
        }
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
}
