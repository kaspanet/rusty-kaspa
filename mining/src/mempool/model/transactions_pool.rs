use super::{
    super::{
        config::Config,
        errors::{RuleError, RuleResult},
        model::{map::IdToTransactionMap, tx::MempoolTransaction},
    },
    map::TransactionIdSet,
    pool::Pool,
    utxo_set::MempoolUtxoSet,
};
use consensus_core::{api::DynConsensus, tx::MutableTransaction, tx::TransactionId};
use kaspa_core::{debug, warn};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::SystemTime,
};

type ParentTransactionIdsInPool = HashMap<TransactionId, TransactionIdSet>;
type ChainedTransactionIdsByParentId = HashMap<TransactionId, TransactionIdSet>;

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
///   requiring smart pointers eventually.
pub(crate) struct TransactionsPool {
    consensus: DynConsensus,
    config: Rc<Config>,
    pub(crate) all_transactions: IdToTransactionMap,
    parent_transaction_ids_in_pool: ParentTransactionIdsInPool,
    chained_transaction_ids_by_parent_id: ChainedTransactionIdsByParentId,
    last_expire_scan_daa_score: u64,
    /// last expire scan time in milliseconds
    last_expire_scan_time: u64,
}

impl TransactionsPool {
    pub(crate) fn new(consensus: DynConsensus, config: Rc<Config>) -> Self {
        Self {
            consensus,
            config,
            all_transactions: IdToTransactionMap::default(),
            parent_transaction_ids_in_pool: ParentTransactionIdsInPool::default(),
            chained_transaction_ids_by_parent_id: ChainedTransactionIdsByParentId::default(),
            last_expire_scan_daa_score: 0,
            last_expire_scan_time: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
        }
    }

    pub(crate) fn _consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }

    pub(crate) fn add_transaction(
        &mut self,
        mempool_utxo_set: &mut MempoolUtxoSet,
        transaction: MutableTransaction,
        is_high_priority: bool,
    ) -> RuleResult<&MempoolTransaction> {
        let virtual_daa_score = self.consensus.clone().get_virtual_daa_score();
        let transaction = MempoolTransaction::new(transaction, is_high_priority, virtual_daa_score);
        let id = transaction.id();
        self.add_mempool_transaction(mempool_utxo_set, transaction)?;
        Ok(self.get(&id).unwrap())
    }

    pub(crate) fn add_mempool_transaction(
        &mut self,
        mempool_utxo_set: &mut MempoolUtxoSet,
        transaction: MempoolTransaction,
    ) -> RuleResult<()> {
        // The call to get_parent_transaction_ids_in_pool is a tradeoff:
        // validateAndInsertTransaction has the collection but process_orphans_after_accepted_transaction has not.
        // So we build the collection in-place here.
        let id = transaction.id();
        self.parent_transaction_ids_in_pool.insert(id, self.get_parent_transaction_ids_in_pool(&transaction.mtx));
        for parent_transaction_id in self.parent_transaction_ids_in_pool.get(&id).unwrap() {
            self.chained_transaction_ids_by_parent_id.entry(*parent_transaction_id).or_default().insert(id);
        }
        mempool_utxo_set.add_transaction(&transaction.mtx);
        self.all_transactions.insert(id, transaction);
        Ok(())
    }

    pub(crate) fn remove_parent_transaction_id_in_pool(&mut self, transaction_id: &TransactionId, parent_id: &TransactionId) -> bool {
        self.parent_transaction_ids_in_pool.get_mut(transaction_id).unwrap().remove(parent_id)
    }

    pub(crate) fn remove_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        self.parent_transaction_ids_in_pool.remove(transaction_id);
        self.chained_transaction_ids_by_parent_id.remove(transaction_id);
        self.all_transactions.remove(transaction_id).ok_or(RuleError::RejectMissingTransaction(*transaction_id))
    }

    pub(crate) fn expire_old_transactions(&mut self) -> RuleResult<()> {
        let virtual_daa_score = self._consensus().get_virtual_daa_score();
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        if virtual_daa_score - self.last_expire_scan_daa_score < self.config.transaction_expire_scan_interval_daa_score
            || now - self.last_expire_scan_time < self.config.transaction_expire_scan_interval_milliseconds
        {
            return Ok(());
        }

        // Never expire high priority transactions
        // Remove all transactions whose addedAtDAAScore is older then TransactionExpireIntervalDAAScore
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

    /// Is the mempool transaction identified by [transaction_id] ready for being inserted in a block template?
    pub(crate) fn _is_transaction_ready(&self, transaction_id: &TransactionId) -> bool {
        self.parent_transaction_ids_in_pool[transaction_id].is_empty()
    }

    pub(crate) fn _all_ready_transactions(&self) -> Vec<MutableTransaction> {
        // The returned transactions are leaving the mempool so they are cloned
        self.all_transactions.values().filter(|x| self._is_transaction_ready(&x.id())).map(|x| x.mtx.clone()).collect()
    }

    pub(crate) fn get_parent_transaction_ids_in_pool(&self, transaction: &MutableTransaction) -> TransactionIdSet {
        let mut parent_transaction_ids = HashSet::with_capacity(transaction.tx.inputs.len());
        for input in transaction.tx.inputs.iter() {
            if self.has(&input.previous_outpoint.transaction_id) {
                parent_transaction_ids.insert(input.previous_outpoint.transaction_id);
            }
        }
        parent_transaction_ids
    }

    pub(crate) fn get_redeemer_ids(&self, transaction_id: &TransactionId) -> TransactionIdSet {
        let mut redeemers = TransactionIdSet::new();
        if let Some(transaction) = self.get(transaction_id) {
            let mut stack = vec![transaction];
            while !stack.is_empty() {
                let transaction = stack.pop().unwrap();
                for redeemer_id in self.chained_transaction_ids_by_parent_id.get(&transaction.id()).unwrap() {
                    if let Some(redeemer) = self.get(redeemer_id) {
                        if redeemers.insert(*redeemer_id) {
                            stack.push(redeemer);
                        }
                    }
                }
            }
        }
        redeemers
    }

    /// Returns the exceeding low-priority transactions having the lowest fee rates.
    /// An error is returned if the mempool is filled with high priority transactions.
    pub(crate) fn limit_transaction_count(&self) -> RuleResult<Vec<TransactionId>> {
        // Return a vector of transactions to be removed that the caller has to remove actually.
        // The caller is golang validateAndInsertTransaction equivalent.
        // This behavior differs from golang impl.
        let mut transactions_to_remove = Vec::new();
        if self.len() as u64 > self.config.maximum_transaction_count {
            // TODO: consider introducing an index on all_transactions low-priority items instead.
            //
            // Sorting this vector here may be sub-optimal compared with maintaining a sorted
            // index of all_transactions low-priority items if the proportion of low-priority txs
            // in all_transactions is important.
            let mut low_priority_txs = self.all_transactions.values().filter(|x| x.is_high_priority).collect::<Vec<_>>();

            if !low_priority_txs.is_empty() {
                low_priority_txs.sort_by(|a, b| a.fee_rate().partial_cmp(&b.fee_rate()).unwrap());
                transactions_to_remove.extend_from_slice(
                    &low_priority_txs
                        [0..usize::min(self.len() - self.config.maximum_transaction_count as usize, low_priority_txs.len())],
                );
            }
        }

        // An error is returned if the mempool is filled with high priority transactions.
        let tx_count = self.len() - transactions_to_remove.len();
        if tx_count as u64 > self.config.maximum_transaction_count {
            let err = RuleError::RejectMempoolIsFull(tx_count, self.config.maximum_transaction_count);
            warn!("{}", err.to_string());
            return Err(err);
        }

        Ok(transactions_to_remove.iter().map(|x| x.id()).collect())
    }

    // pub(crate) fn get_transactions_by_addresses(&self) -> RuleResult<IOScriptToTransaction> {
    //     todo!()
    // }

    pub(crate) fn get_all_transactions(&self) -> Vec<MutableTransaction> {
        self.all().values().map(|x| x.mtx.clone()).collect()
    }
}

impl Pool for TransactionsPool {
    fn all(&self) -> &IdToTransactionMap {
        &self.all_transactions
    }
}
