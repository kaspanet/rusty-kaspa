use std::collections::HashSet;

use crate::mempool::{
    errors::RuleResult,
    model::{map::OutpointIndex, tx::DoubleSpend},
};
use kaspa_consensus_core::tx::{MutableTransaction, TransactionId, TransactionOutpoint};

pub(crate) struct MempoolOutpointSpenders {
    outpoint_owner_id: OutpointIndex,
}

impl MempoolOutpointSpenders {
    pub(crate) fn new() -> Self {
        Self { outpoint_owner_id: OutpointIndex::default() }
    }

    pub(crate) fn add_transaction(&mut self, transaction: &MutableTransaction) {
        let transaction_id = transaction.id();

        for input in transaction.tx.inputs.iter() {
            self.outpoint_owner_id.insert(input.previous_outpoint, transaction_id);
        }
    }

    pub(crate) fn remove_transaction(&mut self, transaction: &MutableTransaction) {
        for input in transaction.tx.inputs.iter() {
            self.outpoint_owner_id.remove(&input.previous_outpoint);
        }
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.outpoint_owner_id.get(outpoint)
    }

    /// Make sure no other transaction in the mempool is already spending an output which one of this transaction inputs spends
    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        match self.get_first_double_spend(transaction) {
            Some(double_spend) => Err(double_spend.into()),
            None => Ok(()),
        }
    }

    pub(crate) fn get_first_double_spend(&self, transaction: &MutableTransaction) -> Option<DoubleSpend> {
        let transaction_id = transaction.id();
        for input in transaction.tx.inputs.iter() {
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.previous_outpoint)
                && *existing_transaction_id != transaction_id
            {
                return Some(DoubleSpend::new(input.previous_outpoint, *existing_transaction_id));
            }
        }
        None
    }

    /// Returns the first double spend of every transaction in the mempool double spending on `transaction`
    pub(crate) fn get_double_spend_transaction_ids(&self, transaction: &MutableTransaction) -> Vec<DoubleSpend> {
        let transaction_id = transaction.id();
        let mut double_spends = vec![];
        let mut visited = HashSet::new();
        for input in transaction.tx.inputs.iter() {
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.previous_outpoint)
                && *existing_transaction_id != transaction_id
                && visited.insert(*existing_transaction_id)
            {
                double_spends.push(DoubleSpend::new(input.previous_outpoint, *existing_transaction_id));
            }
        }
        double_spends
    }
}
