use crate::mempool::{errors::RuleResult, Mempool};
use kaspa_consensus_core::{api::ConsensusApi, tx::Transaction};
use std::collections::HashSet;

use super::model::{pool::Pool, tx::MempoolTransaction};

impl Mempool {
    pub(crate) fn handle_new_block_transactions(&mut self, block_transactions: &[Transaction]) -> RuleResult<Vec<MempoolTransaction>> {
        let mut unorphaned_transactions = vec![];
        for transaction in block_transactions[1..].iter() {
            let transaction_id = transaction.id();
            self.remove_transaction(&transaction_id, false)?;
            self.remove_double_spends(transaction)?;
            self.orphan_pool.remove_orphan(&transaction_id, false, "accepted")?;
            unorphaned_transactions.append(&mut self.get_unorphaned_transactions_after_accepted_transaction(transaction));
        }
        Ok(unorphaned_transactions)
    }

    pub(crate) fn expire_low_priority_transactions(&mut self, consensus: &dyn ConsensusApi) -> RuleResult<()> {
        self.orphan_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())?;
        self.transaction_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())?;
        Ok(())
    }

    fn remove_double_spends(&mut self, transaction: &Transaction) -> RuleResult<()> {
        let mut transactions_to_remove = HashSet::new();
        for input in transaction.inputs.iter() {
            if let Some(redeemer_id) = self.transaction_pool.get_outpoint_owner_id(&input.previous_outpoint) {
                transactions_to_remove.insert(*redeemer_id);
            }
        }
        transactions_to_remove
            .iter()
            .try_for_each(|x| self.remove_transaction(x, true, "double spend", format!(" favouring {}", transaction.id()).as_str()))
    }
}
