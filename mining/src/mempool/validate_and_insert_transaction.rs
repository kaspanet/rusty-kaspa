use std::sync::Arc;

use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::{pool::Pool, tx::MempoolTransaction},
    Mempool,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
};
use kaspa_core::info;
use kaspa_utils::vec::VecExtensions;

use super::tx::{Orphan, Priority};

impl Mempool {
    pub(crate) fn validate_and_insert_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        transaction: Transaction,
        priority: Priority,
        orphan: Orphan,
    ) -> RuleResult<Vec<Arc<Transaction>>> {
        self.validate_and_insert_mutable_transaction(consensus, MutableTransaction::from_tx(transaction), priority, orphan)
    }

    pub(crate) fn validate_and_insert_mutable_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        mut transaction: MutableTransaction,
        priority: Priority,
        orphan: Orphan,
    ) -> RuleResult<Vec<Arc<Transaction>>> {
        // Populate mass in the beginning, it will be used in multiple places throughout the validation and insertion.
        transaction.calculated_mass = Some(consensus.calculate_transaction_mass(&transaction.tx));

        self.validate_transaction_pre_utxo_entry(&transaction)?;

        match self.populate_entries_and_try_validate(consensus, &mut transaction) {
            Ok(_) => {}
            Err(RuleError::RejectMissingOutpoint) => {
                if orphan == Orphan::Forbidden {
                    return Err(RuleError::RejectDisallowedOrphan(transaction.id()));
                }
                self.orphan_pool.try_add_orphan(consensus, transaction, priority)?;
                return Ok(vec![]);
            }
            Err(err) => {
                return Err(err);
            }
        }

        self.validate_transaction_in_context(&transaction)?;

        // Before adding the transaction, check if there is room in the pool
        self.transaction_pool.limit_transaction_count(1)?.iter().try_for_each(|x| self.remove_transaction(x, true))?;

        // Here the accepted transaction is cloned in order to prevent having self borrowed immutably for the
        // transaction reference and mutably for the call to process_orphans_after_accepted_transaction
        let accepted_transaction =
            self.transaction_pool.add_transaction(transaction, consensus.get_virtual_daa_score(), priority)?.mtx.tx.clone();
        let mut accepted_transactions = self.process_orphans_after_accepted_transaction(consensus, &accepted_transaction)?;
        // We include the original accepted transaction as well
        accepted_transactions.swap_insert(0, accepted_transaction);
        Ok(accepted_transactions)
    }

    fn validate_transaction_pre_utxo_entry(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        self.validate_transaction_in_isolation(transaction)?;
        self.transaction_pool.check_double_spends(transaction)
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

    /// Finds all transactions that can be unorphaned after a some transaction
    /// has been accepted. Unorphan and add those to the transaction pool.
    ///
    /// Returns the list of all successfully processed transactions.
    pub(crate) fn process_orphans_after_accepted_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        accepted_transaction: &Transaction,
    ) -> RuleResult<Vec<Arc<Transaction>>> {
        // Rust rewrite:
        // - The function is relocated from OrphanPool into Mempool
        let unorphaned_transactions = self.get_unorphaned_transactions_after_accepted_transaction(consensus, accepted_transaction)?;
        let mut added_transactions = Vec::with_capacity(unorphaned_transactions.len() + 1); // +1 since some callers add the accepted tx itself
        for transaction in unorphaned_transactions {
            // The returned transactions are leaving the mempool but must also be added to
            // the transaction pool so we clone.
            added_transactions.push(transaction.mtx.tx.clone());
            self.transaction_pool.add_mempool_transaction(transaction)?;
        }
        Ok(added_transactions)
    }

    /// Returns a list with all successfully unorphaned transactions after some
    /// transaction has been accepted.
    fn get_unorphaned_transactions_after_accepted_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        transaction: &Transaction,
    ) -> RuleResult<Vec<MempoolTransaction>> {
        let mut accepted_orphans = Vec::new();
        let transaction_id = transaction.id();
        let mut outpoint = TransactionOutpoint::new(transaction_id, 0);
        for (i, output) in transaction.outputs.iter().enumerate() {
            outpoint.index = i as u32;
            let mut orphan_id = None;
            if let Some(orphan) = self.orphan_pool.outpoint_orphan_mut(&outpoint) {
                for (i, input) in orphan.mtx.tx.inputs.iter().enumerate() {
                    if input.previous_outpoint == outpoint {
                        if orphan.mtx.entries[i].is_none() {
                            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false);
                            orphan.mtx.entries[i] = Some(entry);
                            if orphan.mtx.is_verifiable() {
                                orphan_id = Some(orphan.id());
                            }
                        }
                        break;
                    }
                }
            } else {
                continue;
            }
            if let Some(orphan_id) = orphan_id {
                match self.unorphan_transaction(consensus, &orphan_id) {
                    Ok(accepted_tx) => {
                        accepted_orphans.push(accepted_tx);
                    }
                    Err(err) => {
                        // In case of validation error, we log the problem and drop the
                        // erroneous transaction.
                        info!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err.to_string());
                    }
                }
            }
        }
        Ok(accepted_orphans)
    }

    fn unorphan_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        transaction_id: &TransactionId,
    ) -> RuleResult<MempoolTransaction> {
        // Rust rewrite:
        // - Instead of adding the validated transaction to mempool transaction pool,
        //   we return it.
        // - The function is relocated from OrphanPool into Mempool

        // Remove the transaction identified by transaction_id from the orphan pool.
        let mut transactions = self.orphan_pool.remove_orphan(transaction_id, false)?;

        // At this point, `transactions` contain exactly one transaction.
        // The one we just removed from the orphan pool.
        assert_eq!(transactions.len(), 1, "the list returned by remove_orphan is expected to contain exactly one transaction");
        let mut transaction = transactions.pop().unwrap();

        consensus.validate_mempool_transaction_and_populate(&mut transaction.mtx)?;
        self.validate_transaction_in_context(&transaction.mtx)?;
        transaction.added_at_daa_score = consensus.get_virtual_daa_score();
        Ok(transaction)
    }
}
