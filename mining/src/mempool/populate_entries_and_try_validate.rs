use crate::mempool::{errors::RuleResult, model::pool::Pool, Mempool};
use kaspa_consensus_core::{api::ConsensusApi, constants::UNACCEPTED_DAA_SCORE, tx::MutableTransaction, tx::UtxoEntry};
use kaspa_mining_errors::mempool::RuleError;

impl Mempool {
    pub(crate) fn populate_entries_and_try_validate(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: &mut MutableTransaction,
    ) -> RuleResult<()> {
        // Rust rewrite note:
        // Neither parentsInPool nor missingOutpoints are actually used or needed by the
        // callers so we neither build nor return them.
        // parentsInPool is now built by transactions_pool::add_mempool_transaction.
        // missingOutpoints is reduced to a simple ConsensusError::TxMissingOutpoints.

        self.populate_mempool_entries(transaction);
        validate_mempool_transaction_and_populate(consensus, transaction)?;
        Ok(())
    }

    pub(super) fn populate_mempool_entries(&self, transaction: &mut MutableTransaction) {
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(parent) = self.transaction_pool.get(&input.previous_outpoint.transaction_id) {
                let output = &parent.mtx.tx.outputs[input.previous_outpoint.index as usize];
                transaction.entries[i] =
                    Some(UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false));
            }
        }
    }
}

pub(crate) fn validate_mempool_transaction_and_populate(
    consensus: &dyn ConsensusApi,
    transaction: &mut MutableTransaction,
) -> RuleResult<()> {
    Ok(consensus.validate_mempool_transaction_and_populate(transaction)?)
}

pub(crate) fn validate_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MutableTransaction],
) -> Vec<RuleResult<()>> {
    consensus.validate_mempool_transactions_in_parallel(transactions).into_iter().map(|x| x.map_err(RuleError::from)).collect()
}
