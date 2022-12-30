use super::{errors::RuleResult, model::pool::Pool, Mempool};
use consensus_core::{constants::UNACCEPTED_DAA_SCORE, tx::MutableTransaction, tx::UtxoEntry};

impl Mempool {
    pub(crate) fn fill_inputs_and_get_missing_parents(&self, transaction: &mut MutableTransaction) -> RuleResult<()> {
        // Rust rewrite note:
        // Neither parentsInPool nor missingOutpoints are actually used or needed by the
        // callers so we don't return them.
        // parentsInPool is now built by transactions_pool::add_mempool_transaction.
        // missingOutpoints is reduced to a simple ConsensusError::TxMissingOutpoints.

        self.fill_inputs(transaction);
        self.consensus().validate_mempool_transaction_and_populate(transaction)?;
        Ok(())
    }

    fn fill_inputs(&self, transaction: &mut MutableTransaction) {
        let parent_ids_in_pool = self.transaction_pool.get_parent_transaction_ids_in_pool(transaction);
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if parent_ids_in_pool.contains(&input.previous_outpoint.transaction_id) {
                if let Some(parent) = self.transaction_pool.get(&input.previous_outpoint.transaction_id) {
                    let output = &parent.mtx.tx.outputs[input.previous_outpoint.index as usize];
                    transaction.entries[i] =
                        Some(UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false));
                }
            }
        }
    }
}
