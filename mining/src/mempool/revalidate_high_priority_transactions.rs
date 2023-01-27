use crate::{
    mempool::{
        errors::{RuleError, RuleResult},
        model::pool::Pool,
        Mempool,
    },
    model::topological_index::TopologicalIndex,
};
use consensus_core::tx::{MutableTransaction, TransactionId};
use kaspa_core::debug;

impl Mempool {
    pub(crate) fn revalidate_high_priority_transactions(&mut self) -> RuleResult<Vec<TransactionId>> {
        // First establish a topologically ordered list of all high priority transaction ids

        // Processing the transactions in a parent to chained order guarantees that
        // any transaction removal will propagate to all chained dependencies saving
        // validations calls to consensus.
        let ids = self.transaction_pool.index(true).topological_index()?;
        let mut valid_ids = vec![];

        for transaction_id in ids.iter() {
            // Try to take the transaction out of the storage map so we can mutate it with some self functions.
            // The redeemers of removed transactions are removed too so the following call may return a None.
            if let Some(mut transaction) = self.transaction_pool.all_mut().remove(transaction_id) {
                let is_valid = self.revalidate_transaction(&mut transaction.mtx)?;
                // After mutating we can now put the transaction back into the storage map.
                // The alternative would be to wrap transactions in the pools with a RefCell.
                self.transaction_pool.all_mut().insert(*transaction_id, transaction);
                if is_valid {
                    // A following transaction should not remove this one from the pool since we process
                    // in topological order
                    // TODO: consider the scenario of two high priority txs sandwiching a low one, where
                    // in this case topology order is not guaranteed since we topologically sorted only
                    // high-priority transactions
                    valid_ids.push(*transaction_id);
                } else {
                    debug!("Removing transaction {0}, it failed revalidation", transaction_id);
                    // This call cleanly removes the invalid transaction and its redeemers.
                    self.remove_transaction(transaction_id, true)?;
                }
            }
        }
        // Return the successfully processed high priority transaction ids
        Ok(valid_ids)
    }

    fn revalidate_transaction(&self, transaction: &mut MutableTransaction) -> RuleResult<bool> {
        transaction.clear_entries();
        match self.populate_entries_and_try_validate(transaction) {
            Ok(_) => Ok(true),
            Err(RuleError::RejectMissingOutpoint) => Ok(false),
            Err(err) => Err(err),
        }
    }
}
