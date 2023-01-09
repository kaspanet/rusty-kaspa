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
        // First establish a topologically ordered list of all transaction ids.
        //
        // Processing the transactions in a parent to chained order guarantees that
        // any transaction removal will propagate to all chained dependencies saving
        // validations calls to consensus.
        let ids = self.transaction_pool.topological_index()?;
        for transaction_id in ids.iter() {
            // Try to take the transaction out of the storage map so we can mutate it with some self functions.
            // The redeemers of removed transactions are removed too so the following call may return a None.
            if let Some(mut transaction) = self.transaction_pool.all_mut().remove(transaction_id) {
                // Only high priority transactions are revalidated.
                // TODO: consider revalidating all transaction types
                let mut validation_result = Ok(true);
                if transaction.is_high_priority {
                    validation_result = self.revalidate_transaction(&mut transaction.mtx);
                }
                // Then put the transaction back into the storage map.
                self.transaction_pool.all_mut().insert(*transaction_id, transaction);
                if !validation_result? {
                    debug!("Removing transaction {0}, it failed revalidation", transaction_id);
                    // This call cleanly removes the invalid transaction and its redeemers.
                    self.remove_transaction(transaction_id, true)?;
                }
            }
        }
        // Return the mempool remaining valid high priority transaction ids
        Ok(self.transaction_pool.all().values().filter_map(|x| if x.is_high_priority { Some(x.id()) } else { None }).collect())
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
