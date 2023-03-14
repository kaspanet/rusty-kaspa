use crate::{
    consensus_context::ConsensusMiningContext,
    model::{
        candidate_tx::CandidateTransaction,
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
    },
};

use self::{
    config::Config,
    model::{orphan_pool::OrphanPool, pool::Pool, transactions_pool::TransactionsPool},
};
use consensus_core::tx::{MutableTransaction, TransactionId};
use std::sync::Arc;

pub(crate) mod check_transaction_standard;
pub mod config;
pub mod errors;
pub(crate) mod handle_new_block_transactions;
mod model;
pub(crate) mod populate_entries_and_try_validate;
pub(crate) mod remove_transaction;
pub(crate) mod revalidate_high_priority_transactions;
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
pub(crate) struct Mempool<T: ConsensusMiningContext + ?Sized> {
    config: Arc<Config>,
    consensus: Arc<T>,
    transaction_pool: TransactionsPool<T>,
    orphan_pool: OrphanPool<T>,
}

impl<T: ConsensusMiningContext + ?Sized> Mempool<T> {
    pub(crate) fn new(consensus: Arc<T>, config: Config) -> Self {
        let config = Arc::new(config);
        let transaction_pool = TransactionsPool::new(consensus.clone(), config.clone());
        let orphan_pool = OrphanPool::new(consensus.clone(), config.clone());
        Self { config, consensus, transaction_pool, orphan_pool }
    }

    pub(crate) fn consensus(&self) -> &T {
        &self.consensus
    }

    pub(crate) fn get_transaction(
        &self,
        transaction_id: &TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> Option<MutableTransaction> {
        let mut transaction = None;
        if include_transaction_pool {
            transaction = self.transaction_pool.all().get(transaction_id);
        }
        if transaction.is_none() && include_orphan_pool {
            transaction = self.orphan_pool.get(transaction_id);
        }
        transaction.map(|x| x.mtx.clone())
    }

    pub(crate) fn get_all_transactions(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        let mut transactions = vec![];
        let mut orphans = vec![];
        if include_transaction_pool {
            transactions = self.transaction_pool.get_all_transactions()
        }
        if include_orphan_pool {
            orphans = self.orphan_pool.get_all_transactions()
        }
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
        self.transaction_pool.all_ready_transactions()
    }
}
