use consensus_core::{
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, TransactionId, VerifiableTransaction},
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};

use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::map::OutpointToIdMap,
};

use super::{pool::Pool, transactions_pool::TransactionsPool};

pub(crate) struct MempoolUtxoSet {
    pool_unspent_outputs: UtxoCollection,
    outpoint_owner_id: OutpointToIdMap,
}

impl MempoolUtxoSet {
    pub(crate) fn new() -> Self {
        Self { pool_unspent_outputs: UtxoCollection::default(), outpoint_owner_id: OutpointToIdMap::default() }
    }

    pub(crate) fn add_transaction(&mut self, transaction: &MutableTransaction) {
        let transaction_id = transaction.id();
        let mut outpoint = TransactionOutpoint { transaction_id, index: 0 };

        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            outpoint.index = i as u32;

            // Delete the output this input spends, in case it was created by mempool.
            // If the outpoint doesn't exist in self.pool_unspent_outputs - this means
            // it was created in the DAG (a.k.a. in consensus).
            self.pool_unspent_outputs.remove(&outpoint);

            self.outpoint_owner_id.insert(input.previous_outpoint, transaction_id);
        }

        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint { transaction_id, index: i as u32 };
            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false);
            self.pool_unspent_outputs.insert(outpoint, entry);
        }
    }

    pub(crate) fn remove_transaction(&mut self, transaction_pool: &TransactionsPool, transaction: &MutableTransaction) {
        let transaction_id = transaction.id();
        for (input, entry) in transaction.as_verifiable().populated_inputs() {
            // If the transaction creating the output spent by this input is in the mempool - restore it's UTXO
            if transaction_pool.get(&input.previous_outpoint.transaction_id).is_some() {
                self.pool_unspent_outputs.insert(input.previous_outpoint, entry.clone());
            }
            self.outpoint_owner_id.remove(&input.previous_outpoint);
        }

        let mut outpoint = TransactionOutpoint { transaction_id, index: 0 };
        for i in 0..transaction.tx.outputs.len() {
            outpoint.index = i as u32;
            self.pool_unspent_outputs.remove(&outpoint);
        }
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.outpoint_owner_id.get(outpoint)
    }

    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        for input in transaction.tx.inputs.iter() {
            if let Some(existing_transaction_id) = self.outpoint_owner_id.get(&input.previous_outpoint) {
                return Err(RuleError::RejectDoubleSpendInMempool(input.previous_outpoint, *existing_transaction_id));
            }
        }
        Ok(())
    }
}
