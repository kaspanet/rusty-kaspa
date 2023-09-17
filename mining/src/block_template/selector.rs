use kaspa_core::{time::Stopwatch, trace};
use rand::Rng;
use std::{
    collections::{HashMap, HashSet},
    vec,
};

use crate::model::candidate_tx::CandidateTransaction;

use super::{
    model::tx::{CandidateList, SelectableTransaction, SelectableTransactions, TransactionIndex},
    policy::Policy,
};
use kaspa_consensus_core::{
    subnets::SubnetworkId,
    tx::{Transaction, TransactionId},
};

/// ALPHA is a coefficient that defines how uniform the distribution of
/// candidate transactions should be. A smaller alpha makes the distribution
/// more uniform. ALPHA is used when determining a candidate transaction's
/// initial p value.
const ALPHA: i32 = 3;

/// REBALANCE_THRESHOLD is the percentage of candidate transactions under which
/// we don't rebalance. Rebalancing is a heavy operation so we prefer to avoid
/// rebalancing very often. On the other hand, if we don't rebalance often enough
/// we risk having too many collisions.
/// The value is derived from the max probability of collision. That is to say,
/// if REBALANCE_THRESHOLD is 0.95, there's a 1-in-20 chance of collision.
const REBALANCE_THRESHOLD: f64 = 0.95;

pub(crate) struct TransactionsSelector {
    policy: Policy,
    /// Transaction store
    transactions: Vec<CandidateTransaction>,
    /// Selectable transactions store
    selectable_txs: SelectableTransactions,

    /// Indexes of transactions keys in stores
    rejected_txs: HashSet<TransactionId>,

    /// Indexes of selected transactions in stores
    selected_txs: Vec<TransactionIndex>,
    total_mass: u64,
    total_fees: u64,
}

impl TransactionsSelector {
    pub(crate) fn new(policy: Policy, mut transactions: Vec<CandidateTransaction>) -> Self {
        let _sw = Stopwatch::<100>::with_threshold("TransactionsSelector::new op");
        // Sort the transactions by subnetwork_id.
        transactions.sort_by(|a, b| a.tx.subnetwork_id.cmp(&b.tx.subnetwork_id));

        // Create the object without selectable transactions
        let mut selector = Self {
            policy,
            transactions,
            selectable_txs: vec![],
            rejected_txs: Default::default(),
            selected_txs: vec![],
            total_mass: 0,
            total_fees: 0,
        };

        // Create the selectable transactions
        selector.selectable_txs =
            selector.transactions.iter().map(|x| SelectableTransaction::new(selector.calc_tx_value(x), 0, ALPHA)).collect();

        selector
    }

    pub(crate) fn len(&self) -> usize {
        self.transactions.len() - self.rejected_txs.len()
    }

    /// select_transactions implements a probabilistic transaction selection algorithm.
    /// The algorithm, roughly, is as follows:
    /// 1. We assign a probability to each transaction equal to:
    ///    (candidateTx.Value^alpha) / Σ(tx.Value^alpha)
    ///    Where the sum of the probabilities of all txs is 1.
    /// 2. We draw a random number in [0,1) and select a transaction accordingly.
    /// 3. If it's valid, add it to the selectedTxs and remove it from the candidates.
    /// 4. Continue iterating the above until we have either selected all
    ///    available transactions or ran out of gas/block space.
    ///
    /// Note that we make two optimizations here:
    /// * Draw a number in [0,Σ(tx.Value^alpha)) to avoid normalization
    /// * Instead of removing a candidate after each iteration, mark it for deletion.
    ///   Once the sum of probabilities of marked transactions is greater than
    ///   REBALANCE_THRESHOLD percent of the sum of probabilities of all transactions,
    ///   rebalance.
    ///
    /// select_transactions loops over the candidate transactions
    /// and appends the ones that will be included in the next block into
    /// selected_txs.
    pub(crate) fn select_transactions(&mut self) -> Vec<Transaction> {
        let _sw = Stopwatch::<15>::with_threshold("select_transaction op");
        let mut rng = rand::thread_rng();

        self.reset();
        let mut candidate_list = CandidateList::new(&self.selectable_txs);
        let mut used_count = 0;
        let mut used_p = 0.0;
        let mut gas_usage_map: HashMap<SubnetworkId, u64> = HashMap::new();

        while candidate_list.candidates.len() - used_count > 0 {
            // Rebalance the candidates if it's required
            if used_p >= REBALANCE_THRESHOLD * candidate_list.total_p {
                candidate_list = candidate_list.rebalanced(&self.selectable_txs);
                used_count = 0;
                used_p = 0.0;

                // Break if we now ran out of transactions
                if candidate_list.is_empty() {
                    break;
                }
            }

            // Select a candidate tx at random
            let r = rng.gen::<f64>() * candidate_list.total_p;
            let selected_candidate_idx = candidate_list.find(r);
            let selected_candidate = candidate_list.candidates.get_mut(selected_candidate_idx).unwrap();

            // If is_marked_for_deletion is set, it means we got a collision.
            // Ignore and select another Tx.
            if selected_candidate.is_marked_for_deletion {
                continue;
            }
            let selected_tx = &self.transactions[selected_candidate.index];

            // Enforce maximum transaction mass per block.
            // Also check for overflow.
            let next_total_mass = self.total_mass.checked_add(selected_tx.calculated_mass);
            if next_total_mass.is_none() || next_total_mass.unwrap() > self.policy.max_block_mass {
                trace!("Tx {0} would exceed the max block mass. As such, stopping.", selected_tx.tx.id());
                break;
            }

            // Enforce maximum gas per subnetwork per block.
            // Also check for overflow.
            if !selected_tx.tx.subnetwork_id.is_builtin_or_native() {
                let subnetwork_id = selected_tx.tx.subnetwork_id.clone();
                let gas_usage = gas_usage_map.entry(subnetwork_id.clone()).or_insert(0);
                let tx_gas = selected_tx.tx.gas;
                let next_gas_usage = (*gas_usage).checked_add(tx_gas);
                if next_gas_usage.is_none() || next_gas_usage.unwrap() > self.selectable_txs[selected_candidate.index].gas_limit {
                    trace!(
                        "Tx {0} would exceed the gas limit in subnetwork {1}. Removing all remaining txs from this subnetwork.",
                        selected_tx.tx.id(),
                        subnetwork_id
                    );
                    for i in selected_candidate_idx..candidate_list.candidates.len() {
                        let transaction_index = candidate_list.candidates[i].index;
                        // candidateTxs are ordered by subnetwork, so we can safely assume
                        // that transactions after subnetworkID will not be relevant.
                        if subnetwork_id < self.transactions[transaction_index].tx.subnetwork_id {
                            break;
                        }
                        let current = candidate_list.candidates.get_mut(i).unwrap();

                        // Mark for deletion
                        current.is_marked_for_deletion = true;
                        used_count += 1;
                        used_p += self.selectable_txs[transaction_index].p;
                    }
                    continue;
                }
                // Here we know that next_gas_usage is some (since no overflow occurred) so we can safely unwrap.
                *gas_usage = next_gas_usage.unwrap();
            }

            // Add the transaction to the result, increment counters, and
            // save the masses, fees, and signature operation counts to the
            // result.
            self.selected_txs.push(selected_candidate.index);
            self.total_mass += selected_tx.calculated_mass;
            self.total_fees += selected_tx.calculated_fee;

            trace!(
                "Adding tx {0} (feePerMegaGram {1})",
                selected_tx.tx.id(),
                selected_tx.calculated_fee * 1_000_000 / selected_tx.calculated_mass
            );

            // Mark for deletion
            selected_candidate.is_marked_for_deletion = true;
            used_count += 1;
            used_p += self.selectable_txs[selected_candidate.index].p;
        }

        self.selected_txs.sort();

        self.get_transactions()
    }

    fn get_transactions(&self) -> Vec<Transaction> {
        // These transactions leave the selector so we clone
        self.selected_txs.iter().map(|x| self.transactions[*x].tx.as_ref().clone()).collect()
    }

    pub(crate) fn reject(&mut self, transaction_id: TransactionId) {
        self.rejected_txs.insert(transaction_id);
    }

    fn commit_rejections(&mut self) {
        let _sw = Stopwatch::<5>::with_threshold("commit_rejections op");
        if self.rejected_txs.is_empty() {
            return;
        }
        for (index, tx) in self.transactions.iter().enumerate() {
            if !self.selectable_txs[index].is_rejected && self.rejected_txs.remove(&tx.tx.id()) {
                self.selectable_txs[index].is_rejected = true;
                if self.rejected_txs.is_empty() {
                    break;
                }
            }
        }
    }

    fn reset(&mut self) {
        assert_eq!(self.transactions.len(), self.selectable_txs.len());
        self.selected_txs = Vec::with_capacity(self.transactions.len());
        self.commit_rejections();
    }

    /// calc_tx_value calculates a value to be used in transaction selection.
    /// The higher the number the more likely it is that the transaction will be
    /// included in the block.
    fn calc_tx_value(&self, transaction: &CandidateTransaction) -> f64 {
        let mass_limit = self.policy.max_block_mass as f64;
        let mass = transaction.calculated_mass as f64;
        let fee = transaction.calculated_fee as f64;
        if transaction.tx.subnetwork_id.is_builtin_or_native() {
            fee / mass / mass_limit
        } else {
            // TODO: Replace with real gas once implemented
            let gas_limit = u64::MAX as f64;
            fee / mass / mass_limit + transaction.tx.gas as f64 / gas_limit
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: add unit-tests for select_transactions
}
