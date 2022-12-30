use super::{
    errors::{RuleError, RuleResult},
    model::pool::Pool,
    Mempool,
};
use consensus_core::tx::{MutableTransaction, Transaction};

impl Mempool {
    pub(crate) fn validate_and_insert_transaction(
        &mut self,
        mut transaction: MutableTransaction,
        is_high_priority: bool,
        allow_orphan: bool,
    ) -> RuleResult<Vec<MutableTransaction>> {
        // Populate mass in the beginning, it will be used in multiple places throughout the validation and insertion.
        transaction.calculated_mass = Some(self.consensus().calculate_transaction_mass(&transaction.tx));

        self.validate_transaction_pre_utxo_entry(&transaction)?;

        match self.fill_inputs_and_get_missing_parents(&mut transaction) {
            Ok(_) => {}
            Err(RuleError::RejectMissingOutpoint) => {
                if !allow_orphan {
                    return Err(RuleError::RejectDisallowedOrphan(transaction.id()));
                }
                self.orphan_pool.maybe_add_orphan(transaction, is_high_priority)?;
                return Ok(vec![]);
            }
            Err(err) => {
                return Err(err);
            }
        }

        self.validate_transaction_in_context(&transaction)?;

        // TODO: find a way to avoid the clone
        let accepted_transaction =
            self.transaction_pool.add_transaction(&mut self.mempool_utxo_set, transaction, is_high_priority)?.mtx.tx.clone();

        let accepted_orphans = self.process_orphans_after_accepted_transaction(&accepted_transaction)?;
        self.transaction_pool.limit_transaction_count()?.iter().try_for_each(|x| self.remove_transaction(x, true))?;
        Ok(accepted_orphans)
    }

    fn validate_transaction_pre_utxo_entry(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        self.validate_transaction_in_isolation(transaction)?;
        self.mempool_utxo_set.check_double_spends(transaction)
    }

    fn validate_transaction_in_isolation(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        let transaction_id = transaction.id();
        if self.transaction_pool.has(&transaction_id) {
            return Err(RuleError::RejectDuplicate(transaction_id));
        }
        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_isolation(transaction)?;
        }
        Ok(())
    }

    fn validate_transaction_in_context(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_context(transaction)?;
        }
        Ok(())
    }

    pub(crate) fn process_orphans_after_accepted_transaction(
        &mut self,
        accepted_transaction: &Transaction,
    ) -> RuleResult<Vec<MutableTransaction>> {
        let mut added_transactions = Vec::new();
        let mut unorphaned_transactions = self.orphan_pool.process_orphans_after_accepted_transaction(accepted_transaction)?;
        while !unorphaned_transactions.is_empty() {
            let transaction = unorphaned_transactions.pop().unwrap();

            // The returned transactions are leaving the mempool but must also be added to
            // the transaction pool so we clone.
            added_transactions.push(transaction.mtx.clone());

            self.transaction_pool.add_mempool_transaction(&mut self.mempool_utxo_set, transaction)?;
        }
        Ok(added_transactions)
    }
}
