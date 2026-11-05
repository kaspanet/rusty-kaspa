use std::collections::HashSet;

use crate::{
    mempool::{
        errors::RuleResult,
        model::{map::OutpointIndex, tx::DoubleSpend},
    },
    model::TransactionIdSet,
};
use kaspa_consensus_core::{
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, TransactionId, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};

pub(crate) struct MempoolUtxoSet {
    pool_unspent_outputs: UtxoCollection,
    outpoint_owner_id: OutpointIndex,
}

impl MempoolUtxoSet {
    pub(crate) fn new() -> Self {
        Self { pool_unspent_outputs: UtxoCollection::default(), outpoint_owner_id: OutpointIndex::default() }
    }

    pub(crate) fn add_transaction(&mut self, transaction: &MutableTransaction) {
        let transaction_id = transaction.id();

        for input in transaction.tx.inputs.iter() {
            // Delete the output this input spends, in case it was created by mempool.
            // If the outpoint doesn't exist in self.pool_unspent_outputs - this means
            // it was created in the DAG (a.k.a. in consensus).
            self.pool_unspent_outputs.remove(&input.previous_outpoint);

            self.outpoint_owner_id.insert(input.previous_outpoint, transaction_id);
        }

        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint::new(transaction_id, i as u32);
            let entry = UtxoEntry::new(
                output.value,
                output.script_public_key.clone(),
                UNACCEPTED_DAA_SCORE,
                false,
                output.covenant.map(|x| x.covenant_id),
            );
            self.pool_unspent_outputs.insert(outpoint, entry);
        }
    }

    pub(crate) fn remove_transaction(&mut self, transaction: &MutableTransaction, parent_ids_in_pool: &TransactionIdSet) {
        let transaction_id = transaction.id();
        // We cannot assume here that the transaction is fully populated.
        // Notably, this is not the case when revalidate_transaction fails and leads the execution path here.
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(ref entry) = transaction.entries[i] {
                // If the transaction creating the output spent by this input is in the mempool - restore it's UTXO
                if parent_ids_in_pool.contains(&input.previous_outpoint.transaction_id) {
                    self.pool_unspent_outputs.insert(input.previous_outpoint, entry.clone());
                }
            }
            self.outpoint_owner_id.remove(&input.previous_outpoint);
        }

        let mut outpoint = TransactionOutpoint::new(transaction_id, 0);
        for i in 0..transaction.tx.outputs.len() {
            outpoint.index = i as u32;
            self.pool_unspent_outputs.remove(&outpoint);
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutput},
    };
    use kaspa_hashes::Hash;
    use std::sync::Arc;

    fn spk() -> ScriptPublicKey {
        ScriptPublicKey::from_vec(0, vec![])
    }

    fn mtx(tx: Transaction) -> MutableTransaction {
        MutableTransaction::new(Arc::new(tx))
    }

    /// When a transaction spends an output that was itself created by a mempool
    /// transaction, `add_transaction` must remove that *parent* output from
    /// `pool_unspent_outputs` (keyed by `input.previous_outpoint`), mirroring the
    /// restore performed by `remove_transaction`.
    #[test]
    fn add_transaction_removes_spent_parent_output() {
        let mut set = MempoolUtxoSet::new();

        // Parent tx A: arbitrary input + one output -> creates unspent output (A, 0).
        let parent = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(99), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1000, spk())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let parent_id = parent.id();
        set.add_transaction(&mtx(parent));

        let parent_out = TransactionOutpoint::new(parent_id, 0);
        assert!(set.pool_unspent_outputs.contains_key(&parent_out), "parent output (A,0) should be tracked as unspent");

        // Child tx B spends (A, 0).
        let child = Transaction::new(
            0,
            vec![TransactionInput::new(parent_out, vec![], 0, 0)],
            vec![TransactionOutput::new(900, spk())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        set.add_transaction(&mtx(child));

        assert!(
            !set.pool_unspent_outputs.contains_key(&parent_out),
            "spending (A,0) must remove it from pool_unspent_outputs, but it is still present"
        );
    }
}
