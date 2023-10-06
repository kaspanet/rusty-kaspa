use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::{
        pool::Pool,
        tx::{MempoolTransaction, TxRemovalReason},
    },
    tx::{Orphan, Priority},
    Mempool,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    constants::{SOMPI_PER_KASPA, UNACCEPTED_DAA_SCORE},
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
};
use kaspa_core::{debug, info};
use std::sync::Arc;

impl Mempool {
    pub(crate) fn pre_validate_and_populate_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        mut transaction: MutableTransaction,
    ) -> RuleResult<MutableTransaction> {
        self.validate_transaction_unacceptance(&transaction)?;
        // Populate mass in the beginning, it will be used in multiple places throughout the validation and insertion.
        transaction.calculated_mass = Some(consensus.calculate_transaction_mass(&transaction.tx));
        self.validate_transaction_in_isolation(&transaction)?;
        self.transaction_pool.check_double_spends(&transaction)?;
        self.populate_mempool_entries(&mut transaction);
        Ok(transaction)
    }

    pub(crate) fn post_validate_and_insert_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        validation_result: RuleResult<()>,
        transaction: MutableTransaction,
        priority: Priority,
        orphan: Orphan,
    ) -> RuleResult<Option<Arc<Transaction>>> {
        let transaction_id = transaction.id();

        // First check if the transaction was not already added to the mempool.
        // The case may arise since the execution of the manager public functions is no
        // longer atomic and different code paths may lead to inserting the same transaction
        // concurrently.
        if self.transaction_pool.has(&transaction_id) {
            debug!("Transaction {0} is not post validated since already in the mempool", transaction_id);
            return Ok(None);
        }

        self.validate_transaction_unacceptance(&transaction)?;

        // Re-check double spends since validate_and_insert_transaction is no longer atomic
        self.transaction_pool.check_double_spends(&transaction)?;

        match validation_result {
            Ok(_) => {}
            Err(RuleError::RejectMissingOutpoint) => {
                if orphan == Orphan::Forbidden {
                    return Err(RuleError::RejectDisallowedOrphan(transaction_id));
                }
                self.orphan_pool.try_add_orphan(consensus.get_virtual_daa_score(), transaction, priority)?;
                return Ok(None);
            }
            Err(err) => {
                return Err(err);
            }
        }

        self.validate_transaction_in_context(&transaction)?;

        // Before adding the transaction, check if there is room in the pool
        self.transaction_pool.limit_transaction_count(1, &transaction)?.iter().try_for_each(|x| {
            self.remove_transaction(x, true, TxRemovalReason::MakingRoom, format!(" for {}", transaction_id).as_str())
        })?;

        // Add the transaction to the mempool as a MempoolTransaction and return a clone of the embedded Arc<Transaction>
        let accepted_transaction =
            self.transaction_pool.add_transaction(transaction, consensus.get_virtual_daa_score(), priority)?.mtx.tx.clone();
        Ok(Some(accepted_transaction))
    }

    /// Validates that the transaction wasn't already accepted into the DAG
    fn validate_transaction_unacceptance(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        // Reject if the transaction is registered as an accepted transaction
        let transaction_id = transaction.id();
        match self.accepted_transactions.has(&transaction_id) {
            true => Err(RuleError::RejectAlreadyAccepted(transaction_id)),
            false => Ok(()),
        }
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
        if self.config.block_spam_txs {
            // TEMP: apply go-kaspad mempool dust prevention patch
            // Note: we do not apply the part of the patch which modifies BBT since
            // we do not support BBT on mainnet yet
            let has_coinbase_input = transaction.entries.iter().any(|e| e.as_ref().unwrap().is_coinbase);
            let num_extra_outs = transaction.tx.outputs.len() as i64 - transaction.tx.inputs.len() as i64;
            if !has_coinbase_input
                && num_extra_outs > 2
                && transaction.calculated_fee.unwrap() < num_extra_outs as u64 * SOMPI_PER_KASPA
            {
                kaspa_core::trace!("Rejected spam tx {} from mempool ({} outputs)", transaction.id(), transaction.tx.outputs.len());
                return Err(RuleError::RejectSpamTransaction(transaction.id()));
            }
        }

        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_context(transaction)?;
        }
        Ok(())
    }

    /// Returns a list with all successfully unorphaned transactions after some
    /// transaction has been accepted.
    pub(crate) fn get_unorphaned_transactions_after_accepted_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Vec<MempoolTransaction> {
        let mut unorphaned_transactions = Vec::new();
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
                match self.unorphan_transaction(&orphan_id) {
                    Ok(unorphaned_tx) => {
                        unorphaned_transactions.push(unorphaned_tx);
                        debug!("Transaction {0} unorphaned", transaction_id);
                    }
                    Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                        debug!("Ignoring already accepted transaction {}", transaction_id);
                    }
                    Err(err) => {
                        // In case of validation error, we log the problem and drop the
                        // erroneous transaction.
                        info!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err.to_string());
                    }
                }
            }
        }

        unorphaned_transactions
    }

    fn unorphan_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        // Rust rewrite:
        // - Instead of adding the validated transaction to mempool transaction pool,
        //   we return it.
        // - The function is relocated from OrphanPool into Mempool.
        // - The function no longer validates the transaction in mempool (signatures) nor in context.
        //   This job is delegated to a fn called later in the process (Manager::validate_and_insert_unorphaned_transactions).

        // Remove the transaction identified by transaction_id from the orphan pool.
        let mut transactions = self.orphan_pool.remove_orphan(transaction_id, false, TxRemovalReason::Unorphaned, "")?;

        // At this point, `transactions` contains exactly one transaction.
        // The one we just removed from the orphan pool.
        assert_eq!(transactions.len(), 1, "the list returned by remove_orphan is expected to contain exactly one transaction");
        let transaction = transactions.pop().unwrap();

        self.validate_transaction_unacceptance(&transaction.mtx)?;
        self.transaction_pool.check_double_spends(&transaction.mtx)?;
        Ok(transaction)
    }
}
