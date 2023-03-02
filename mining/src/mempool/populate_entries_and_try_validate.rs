use crate::{
    consensus_context::ConsensusMiningContext,
    mempool::{errors::RuleResult, model::pool::Pool, Mempool},
};
use consensus_core::{constants::UNACCEPTED_DAA_SCORE, tx::MutableTransaction, tx::UtxoEntry};

impl<T: ConsensusMiningContext + ?Sized> Mempool<T> {
    pub(crate) fn populate_entries_and_try_validate(&self, transaction: &mut MutableTransaction) -> RuleResult<()> {
        // Rust rewrite note:
        // Neither parentsInPool nor missingOutpoints are actually used or needed by the
        // callers so we neither build nor return them.
        // parentsInPool is now built by transactions_pool::add_mempool_transaction.
        // missingOutpoints is reduced to a simple ConsensusError::TxMissingOutpoints.

        self.populate_mempool_entries(transaction);
        self.consensus().validate_mempool_transaction_and_populate(transaction)?;
        Ok(())
    }

    fn populate_mempool_entries(&self, transaction: &mut MutableTransaction) {
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(parent) = self.transaction_pool.get(&input.previous_outpoint.transaction_id) {
                let output = &parent.mtx.tx.outputs[input.previous_outpoint.index as usize];
                transaction.entries[i] =
                    Some(UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false));
            }
        }
    }
}
