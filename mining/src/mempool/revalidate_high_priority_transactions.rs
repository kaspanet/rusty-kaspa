use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::pool::Pool,
    Mempool,
};
use consensus_core::tx::MutableTransaction;
use kaspa_core::debug;

impl Mempool {
    pub(crate) fn revalidate_high_priority_transactions(&mut self) -> RuleResult<Vec<MutableTransaction>> {
        // First make a list of all high priority transaction ids
        let ids = self
            .transaction_pool
            .all()
            .values()
            .filter_map(|x| if x.is_high_priority { Some(x.id()) } else { None })
            .collect::<Vec<_>>();
        for transaction_id in ids.iter() {
            // Try to take the transaction out of the storage map so we can mutate it with some self functions.
            // The redeemers of removed transactions are removed too so the the following call may return a None.
            if let Some(mut transaction) = self.transaction_pool.all_mut().remove(transaction_id) {
                let is_valid = self.revalidate_transaction(&mut transaction.mtx)?;
                // Then put the updated transaction back into the storage map.
                self.transaction_pool.all_mut().insert(*transaction_id, transaction);
                if !is_valid {
                    debug!("Removing transaction {0}, it failed revalidation", transaction_id);
                    // This call cleanly removes the invalid transaction and its redeemers.
                    self.remove_transaction(transaction_id, true)?;
                }
            }
        }
        // The mempool remaining valid high priority mutable transactions are cloned
        // (because they leave the mempool) and returned.
        Ok(self.transaction_pool.all().values().filter_map(|x| if x.is_high_priority { Some(x.mtx.clone()) } else { None }).collect())
    }

    pub(crate) fn _revalidate_high_priority_transactions_expensive_alternative(&mut self) -> RuleResult<Vec<MutableTransaction>> {
        // The mempool high priority mutable transactions are cloned to satisfy the borrow checker.
        // The clones are revalidated and, depending on result, are either re-injected into or
        // removed from the mempool.
        //
        // This code is for benchmarking only. It it not supposed to be used in production.
        let high_priority_transactions: Vec<MutableTransaction> =
            self.transaction_pool.all().values().filter_map(|x| if x.is_high_priority { Some(x.mtx.clone()) } else { None }).collect();
        for mut transaction in high_priority_transactions {
            let transaction_id = transaction.id();
            if self.transaction_pool.has(&transaction_id) {
                if self.revalidate_transaction(&mut transaction)? {
                    if let Some(mempool_transaction) = self.transaction_pool.all_mut().get_mut(&transaction_id) {
                        mempool_transaction.mtx = transaction;
                    }
                } else {
                    debug!("Removing transaction {0}, it failed revalidation", transaction_id);
                    self.remove_transaction(&transaction_id, true)?;
                }
            }
        }
        // The mempool remaining high priority mutable transactions are cloned
        // (because they leave the mempool) and returned.
        Ok(self.transaction_pool.all().values().filter_map(|x| if x.is_high_priority { Some(x.mtx.clone()) } else { None }).collect())
    }

    fn revalidate_transaction(&self, transaction: &mut MutableTransaction) -> RuleResult<bool> {
        transaction.clear_entries();
        match self.fill_inputs_and_get_missing_parents(transaction) {
            Ok(_) => Ok(true),
            Err(RuleError::RejectMissingOutpoint) => Ok(false),
            Err(err) => Err(err),
        }
    }
}
