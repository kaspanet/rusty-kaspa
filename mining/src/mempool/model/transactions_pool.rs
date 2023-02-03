use crate::{
    mempool::{
        config::Config,
        errors::{RuleError, RuleResult},
        model::{map::MempoolTransactionCollection, pool::Pool, tx::MempoolTransaction, utxo_set::MempoolUtxoSet},
    },
    model::{candidate_tx::CandidateTransaction, topological_index::TopologicalIndex},
};
use consensus_core::{
    api::DynConsensus,
    tx::TransactionId,
    tx::{MutableTransaction, TransactionOutpoint},
};
use kaspa_core::{debug, warn};
use std::{
    collections::{hash_map::Keys, hash_set::Iter},
    sync::Arc,
    time::SystemTime,
};

use super::pool::TransactionsEdges;

/// Pool of transactions to be included in a block template
///
/// ### Rust rewrite notes
///
/// The main design decision is to have [MempoolTransaction]s owned by [all_transactions]
/// without any other external reference so no smart pointer is needed.
///
/// This has following consequences:
///
/// - highPriorityTransactions is dropped in favour of an in-place filtered iterator.
/// - MempoolTransaction.parentTransactionsInPool is moved here and replaced by a map from
///   an id to a set of parent transaction ids introducing an indirection stage when
///   a matching object is required.
/// - chainedTransactionsByParentID maps an id instead of a transaction reference
///   introducing a indirection stage when the matching object is required.
/// - Hash sets are used by parent_transaction_ids_in_pool and chained_transaction_ids_by_parent_id
///   instead of vectors to prevent duplicates.
/// - transactionsOrderedByFeeRate is dropped and replaced by an in-place vector
///   of low-priority transactions sorted by fee rates. This design might eventually
///   prove to be sub-optimal, in which case an index should be implemented, probably
///   requiring smart pointers eventually or an indirection stage too.
pub(crate) struct TransactionsPool {
    consensus: DynConsensus,
    config: Arc<Config>,

    /// Store of transactions
    all_transactions: MempoolTransactionCollection,
    /// Transactions dependencies formed by inputs present in pool - ancestor relations.
    parent_transactions: TransactionsEdges,
    /// Transactions dependencies formed by outputs present in pool - successor relations.
    chained_transactions: TransactionsEdges,

    last_expire_scan_daa_score: u64,
    /// last expire scan time in milliseconds
    last_expire_scan_time: u64,

    /// Store of UTXOs
    utxo_set: MempoolUtxoSet,
}

impl TransactionsPool {
    pub(crate) fn new(consensus: DynConsensus, config: Arc<Config>) -> Self {
        Self {
            consensus,
            config,
            all_transactions: MempoolTransactionCollection::default(),
            parent_transactions: TransactionsEdges::default(),
            chained_transactions: TransactionsEdges::default(),
            last_expire_scan_daa_score: 0,
            last_expire_scan_time: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            utxo_set: MempoolUtxoSet::new(),
        }
    }

    pub(crate) fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }

    /// Add a mutable transaction to the pool
    pub(crate) fn add_transaction(
        &mut self,
        transaction: MutableTransaction,
        is_high_priority: bool,
    ) -> RuleResult<&MempoolTransaction> {
        let virtual_daa_score = self.consensus().get_virtual_daa_score();
        let transaction = MempoolTransaction::new(transaction, is_high_priority, virtual_daa_score);
        let id = transaction.id();
        self.add_mempool_transaction(transaction)?;
        Ok(self.get(&id).unwrap())
    }

    /// Add a mempool transaction to the pool
    pub(crate) fn add_mempool_transaction(&mut self, transaction: MempoolTransaction) -> RuleResult<()> {
        let id = transaction.id();

        assert!(!self.all_transactions.contains_key(&id), "transaction {id} to be added already exists in the transactions pool");
        assert!(transaction.mtx.is_fully_populated(), "transaction {id} to be added in the transactions pool is not fully populated");

        // Create the bijective parent/chained relations.
        // This concerns only the parents of the added transaction.
        // The transactions chained to the added transaction cannot be stored
        // here yet since, by definition, they would have been orphans.
        let parents = self.get_parent_transaction_ids_in_pool(&transaction.mtx);
        self.parent_transactions.insert(id, parents.clone());
        for parent_id in parents {
            let entry = self.chained_mut().entry(parent_id).or_default();
            if !entry.contains(&id) {
                entry.insert(id);
            }
        }

        self.utxo_set.add_transaction(&transaction.mtx);
        self.all_transactions.insert(id, transaction);
        Ok(())
    }

    pub(crate) fn remove_parent_chained_relation_in_pool(
        &mut self,
        transaction_id: &TransactionId,
        parent_id: &TransactionId,
    ) -> bool {
        let mut found = false;
        // Remove the bijective parent/chained relation
        if let Some(parents) = self.parent_transactions.get_mut(transaction_id) {
            found = parents.remove(parent_id);
        }
        if let Some(chains) = self.chained_transactions.get_mut(parent_id) {
            found = chains.remove(transaction_id) || found;
        }
        found
    }

    pub(crate) fn remove_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        // Remove all bijective parent/chained relations
        if let Some(parents) = self.parent_transactions.get(transaction_id) {
            for parent in parents.iter() {
                if let Some(chains) = self.chained_transactions.get_mut(parent) {
                    chains.remove(transaction_id);
                }
            }
        }
        if let Some(chains) = self.chained_transactions.get(transaction_id) {
            for chain in chains.iter() {
                if let Some(parents) = self.parent_transactions.get_mut(chain) {
                    parents.remove(transaction_id);
                }
            }
        }
        self.parent_transactions.remove(transaction_id);
        self.chained_transactions.remove(transaction_id);

        // Remove the transaction itself
        self.all_transactions.remove(transaction_id).ok_or(RuleError::RejectMissingTransaction(*transaction_id))
    }

    pub(crate) fn expire_low_priority_transactions(&mut self) -> RuleResult<()> {
        let virtual_daa_score = self.consensus().get_virtual_daa_score();
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        if virtual_daa_score - self.last_expire_scan_daa_score < self.config.transaction_expire_scan_interval_daa_score
            || now - self.last_expire_scan_time < self.config.transaction_expire_scan_interval_milliseconds
        {
            return Ok(());
        }

        // Never expire high priority transactions
        // Remove all transactions whose added_at_daa_score is older then transaction_expire_interval_daa_score
        let expired_low_priority_transactions: Vec<TransactionId> = self
            .all_transactions
            .values()
            .filter_map(|x| {
                if !x.is_high_priority && virtual_daa_score - x.added_at_daa_score > self.config.transaction_expire_interval_daa_score
                {
                    debug!(
                        "Removing transaction {}, because it expired, DAAScore moved by {}, expire interval: {}",
                        x.id(),
                        virtual_daa_score - x.added_at_daa_score,
                        self.config.transaction_expire_interval_daa_score
                    );
                    Some(x.id())
                } else {
                    None
                }
            })
            .collect();

        for transaction_id in expired_low_priority_transactions.iter() {
            self.remove_transaction(transaction_id)?;
        }

        self.last_expire_scan_daa_score = virtual_daa_score;
        self.last_expire_scan_time = now;
        Ok(())
    }

    /// Is the mempool transaction identified by `transaction_id` ready for being inserted into a block template?
    pub(crate) fn is_transaction_ready(&self, transaction_id: &TransactionId) -> bool {
        if self.all_transactions.contains_key(transaction_id) {
            if let Some(parents) = self.parent_transactions.get(transaction_id) {
                return parents.is_empty();
            }
            return true;
        }
        false
    }

    /// all_ready_transactions returns all fully populated mempool transactions having no parents in the mempool.
    /// These transactions are ready for being inserted in a block template.
    pub(crate) fn all_ready_transactions(&self) -> Vec<CandidateTransaction> {
        // The returned transactions are leaving the mempool so they are cloned
        self.all_transactions
            .values()
            .filter_map(|x| if self.is_transaction_ready(&x.id()) { Some(CandidateTransaction::from_mutable(&x.mtx)) } else { None })
            .collect()
    }

    /// Returns the exceeding low-priority transactions having the lowest fee rates in order
    /// to have room for at least `free_slots` new transactions.
    ///
    /// An error is returned if the mempool is filled with high priority transactions.
    pub(crate) fn limit_transaction_count(&self, free_slots: usize) -> RuleResult<Vec<TransactionId>> {
        // Returns a vector of transactions to be removed that the caller has to remove actually.
        // The caller is golang validateAndInsertTransaction equivalent.
        // This behavior differs from golang impl.
        let mut transactions_to_remove = Vec::new();
        if self.len() + free_slots > self.config.maximum_transaction_count as usize {
            // TODO: consider introducing an index on all_transactions low-priority items instead.
            //
            // Sorting this vector here may be sub-optimal compared with maintaining a sorted
            // index of all_transactions low-priority items if the proportion of low-priority txs
            // in all_transactions is important.
            let mut low_priority_txs = self.all_transactions.values().filter(|x| x.is_high_priority).collect::<Vec<_>>();

            if !low_priority_txs.is_empty() {
                low_priority_txs.sort_by(|a, b| a.fee_rate().partial_cmp(&b.fee_rate()).unwrap());
                transactions_to_remove.extend_from_slice(
                    &low_priority_txs[0..usize::min(
                        self.len() + free_slots - self.config.maximum_transaction_count as usize,
                        low_priority_txs.len(),
                    )],
                );
            }
        }

        // An error is returned if the mempool is filled with high priority transactions.
        let tx_count = self.len() + free_slots - transactions_to_remove.len();
        if tx_count as u64 > self.config.maximum_transaction_count {
            let err = RuleError::RejectMempoolIsFull(tx_count - free_slots, self.config.maximum_transaction_count);
            warn!("{}", err.to_string());
            return Err(err);
        }

        Ok(transactions_to_remove.iter().map(|x| x.id()).collect())
    }

    pub(crate) fn get_all_transactions(&self) -> Vec<MutableTransaction> {
        self.all().values().map(|x| x.mtx.clone()).collect()
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.utxo_set.get_outpoint_owner_id(outpoint)
    }

    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        self.utxo_set.check_double_spends(transaction)
    }

    pub(crate) fn remove_transaction_utxos(&mut self, transaction: &MutableTransaction) {
        let parent_ids = self.get_parent_transaction_ids_in_pool(transaction);
        self.utxo_set.remove_transaction(transaction, &parent_ids)
    }
}

type IterTxId<'a> = Iter<'a, TransactionId>;
type KeysTxId<'a> = Keys<'a, TransactionId, MempoolTransaction>;

impl<'a> TopologicalIndex<'a, KeysTxId<'a>, IterTxId<'a>, TransactionId> for TransactionsPool {
    fn topology_nodes(&'a self) -> KeysTxId<'a> {
        self.all_transactions.keys()
    }

    fn topology_node_edges(&'a self, key: &TransactionId) -> Option<IterTxId<'a>> {
        self.chained_transactions.get(key).map(|x| x.iter())
    }
}

impl Pool for TransactionsPool {
    #[inline]
    fn all(&self) -> &MempoolTransactionCollection {
        &self.all_transactions
    }

    #[inline]
    fn all_mut(&mut self) -> &mut MempoolTransactionCollection {
        &mut self.all_transactions
    }

    #[inline]
    fn chained(&self) -> &TransactionsEdges {
        &self.chained_transactions
    }

    #[inline]
    fn chained_mut(&mut self) -> &mut TransactionsEdges {
        &mut self.chained_transactions
    }
}
