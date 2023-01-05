use super::{map::MempoolTransactionCollection, tx::MempoolTransaction};
use crate::model::owner_txs::{OwnerSetTransactions, ScriptPublicKeySet};
use consensus_core::tx::{MutableTransaction, TransactionId};

pub(crate) trait Pool {
    fn all(&self) -> &MempoolTransactionCollection;

    fn all_mut(&mut self) -> &mut MempoolTransactionCollection;

    fn has(&self, transaction_id: &TransactionId) -> bool {
        self.all().contains_key(transaction_id)
    }

    fn get(&self, transaction_id: &TransactionId) -> Option<&MempoolTransaction> {
        self.all().get(transaction_id)
    }

    /// Returns the number of transactions in the pool
    fn len(&self) -> usize {
        self.all().len()
    }

    /// Returns a vector with clones of all the transactions in the pool.
    fn get_all_transactions(&self) -> Vec<MutableTransaction> {
        self.all().values().map(|x| x.mtx.clone()).collect()
    }

    /// Fills owner transactions for a set of script public keys.
    fn fill_owner_set_transactions(&self, script_public_keys: &ScriptPublicKeySet, owner_set: &mut OwnerSetTransactions) {
        script_public_keys.iter().for_each(|script_public_key| {
            let owner = owner_set.owners.entry(script_public_key.clone()).or_default();

            self.all().iter().for_each(|(id, transaction)| {
                // Sending transactions
                if transaction.mtx.entries.iter().any(|x| x.is_some() && x.as_ref().unwrap().script_public_key == *script_public_key) {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    if !owner.sending_txs.contains(id) {
                        owner.sending_txs.insert(*id);
                    }
                }

                // Receiving transactions
                if transaction.mtx.tx.outputs.iter().any(|x| x.script_public_key == *script_public_key) {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    if !owner.receiving_txs.contains(id) {
                        owner.receiving_txs.insert(*id);
                    }
                }
            });
        });
    }
}
