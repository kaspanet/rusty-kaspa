use std::collections::{hash_set::Iter, HashMap, HashSet};

use super::{map::MempoolTransactionCollection, tx::MempoolTransaction};
use crate::model::{
    owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
    topological_index::TopologicalIndex,
    TransactionIdSet,
};
use kaspa_consensus_core::tx::{MutableTransaction, TransactionId};

pub(crate) type TransactionsEdges = HashMap<TransactionId, TransactionIdSet>;

pub(crate) trait Pool {
    fn all(&self) -> &MempoolTransactionCollection;
    fn all_mut(&mut self) -> &mut MempoolTransactionCollection;

    fn chained(&self) -> &TransactionsEdges;
    fn chained_mut(&mut self) -> &mut TransactionsEdges;

    fn has(&self, transaction_id: &TransactionId) -> bool {
        self.all().contains_key(transaction_id)
    }

    fn get(&self, transaction_id: &TransactionId) -> Option<&MempoolTransaction> {
        self.all().get(transaction_id)
    }

    /// Returns the number of transactions in the pool
    fn len(&self) -> usize {
        self.all().len()
    }

    /// Returns an index over either high or low priority transaction ids which can
    /// in turn be topologically ordered.
    fn index(&self, is_high_priority: bool) -> PoolIndex {
        let transactions: TransactionIdSet =
            self.all().iter().filter_map(|(id, tx)| if tx.is_high_priority == is_high_priority { Some(*id) } else { None }).collect();
        let chained_transactions = transactions
            .iter()
            .filter_map(|id| {
                self.chained()
                    .get(id)
                    .map(|chains| (*id, chains.iter().filter_map(|chain| transactions.get(chain).copied()).collect()))
            })
            .collect();
        PoolIndex::new(transactions, chained_transactions)
    }

    /// Returns the ids of all transactions being parents of `transaction` and existing in the pool.
    fn get_parent_transaction_ids_in_pool(&self, transaction: &MutableTransaction) -> TransactionIdSet {
        let mut parents = HashSet::with_capacity(transaction.tx.inputs.len());
        for input in transaction.tx.inputs.iter() {
            if self.has(&input.previous_outpoint.transaction_id) {
                parents.insert(input.previous_outpoint.transaction_id);
            }
        }
        parents
    }

    /// Returns the ids of all transactions being directly and indirectly chained to `transaction_id`
    /// and existing in the pool.
    fn get_redeemer_ids_in_pool(&self, transaction_id: &TransactionId) -> TransactionIdSet {
        let mut redeemers = TransactionIdSet::new();
        if let Some(transaction) = self.get(transaction_id) {
            let mut stack = vec![transaction];
            while !stack.is_empty() {
                let transaction = stack.pop().unwrap();
                if let Some(chains) = self.chained().get(&transaction.id()) {
                    for redeemer_id in chains {
                        if let Some(redeemer) = self.get(redeemer_id) {
                            // Do no revisit transactions
                            if redeemers.insert(*redeemer_id) {
                                stack.push(redeemer);
                            }
                        }
                    }
                }
            }
        }
        redeemers
    }

    /// Returns a vector with clones of all the transactions in the pool.
    fn get_all_transactions(&self) -> Vec<MutableTransaction> {
        self.all().values().map(|x| x.mtx.clone()).collect()
    }

    /// Fills owner transactions for a set of script public keys.
    fn fill_owner_set_transactions(&self, script_public_keys: &ScriptPublicKeySet, owner_set: &mut GroupedOwnerTransactions) {
        script_public_keys.iter().for_each(|script_public_key| {
            let owner = owner_set.owners.entry(script_public_key.clone()).or_default();

            self.all().iter().for_each(|(id, transaction)| {
                // Sending transactions
                if transaction.mtx.entries.iter().any(|x| x.is_some() && x.as_ref().unwrap().script_public_key == *script_public_key) {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    if !owner.sending_txs.contains(id) {
                        owner.sending_txs.insert(*id);
                    }
                }

                // Receiving transactions
                if transaction.mtx.tx.outputs.iter().any(|x| x.script_public_key == *script_public_key) {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    if !owner.receiving_txs.contains(id) {
                        owner.receiving_txs.insert(*id);
                    }
                }
            });
        });
    }
}

pub(crate) struct PoolIndex {
    transactions: TransactionIdSet,
    chained_transactions: TransactionsEdges,
}

impl PoolIndex {
    pub(crate) fn new(transactions: TransactionIdSet, chained_transactions: TransactionsEdges) -> Self {
        Self { transactions, chained_transactions }
    }
}

type IterTxId<'a> = Iter<'a, TransactionId>;

impl<'a> TopologicalIndex<'a, IterTxId<'a>, IterTxId<'a>, TransactionId> for PoolIndex {
    fn topology_nodes(&'a self) -> IterTxId<'a> {
        self.transactions.iter()
    }

    fn topology_node_edges(&'a self, key: &TransactionId) -> Option<IterTxId<'a>> {
        self.chained_transactions.get(key).map(|x| x.iter())
    }
}
