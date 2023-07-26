use crate::mempool::{errors::RuleResult, Mempool};
use kaspa_consensus_core::{api::ConsensusApi, tx::Transaction};
use std::{collections::HashSet, sync::Arc};

use super::model::pool::Pool;

impl Mempool {
    pub(crate) fn handle_new_block_transactions(
        &mut self,
        consensus: &dyn ConsensusApi,
        block_transactions: &[Transaction],
    ) -> RuleResult<Vec<Arc<Transaction>>> {
        let mut accepted_orphans = vec![];
        for transaction in block_transactions[1..].iter() {
            let transaction_id = transaction.id();
            self.remove_transaction(&transaction_id, false)?;
            self.remove_double_spends(transaction)?;
            self.orphan_pool.remove_orphan(&transaction_id, false)?;
            let mut unorphaned_transactions = self.process_orphans_after_accepted_transaction(consensus, transaction)?;
            accepted_orphans.append(&mut unorphaned_transactions);
        }
        self.orphan_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())?;
        self.transaction_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())?;
        Ok(accepted_orphans)
    }

    fn remove_double_spends(&mut self, transaction: &Transaction) -> RuleResult<()> {
        let mut transactions_to_remove = HashSet::new();
        for input in transaction.inputs.iter() {
            if let Some(redeemer_id) = self.transaction_pool.get_outpoint_owner_id(&input.previous_outpoint) {
                transactions_to_remove.insert(*redeemer_id);
            }
        }
        transactions_to_remove.iter().try_for_each(|x| self.remove_transaction(x, true))
    }
}
