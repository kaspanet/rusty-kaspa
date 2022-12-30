use super::{errors::RuleResult, model::pool::Pool, Mempool};
use consensus_core::{tx::MutableTransaction, tx::TransactionId};

impl Mempool {
    pub(crate) fn _remove_transactions(&mut self, transactions: &[MutableTransaction], remove_redeemers: bool) -> RuleResult<()> {
        transactions.iter().try_for_each(|x| self.remove_transaction(&x.id(), remove_redeemers))
    }

    pub(crate) fn remove_transaction(&mut self, transaction_id: &TransactionId, remove_redeemers: bool) -> RuleResult<()> {
        if self.orphan_pool.has(transaction_id) {
            return self.orphan_pool.remove_orphan(transaction_id, true).map(|_| ());
        }

        let mut transactions_to_remove = vec![*transaction_id];
        let redeemers = self.transaction_pool.get_redeemer_ids(transaction_id);
        if remove_redeemers {
            transactions_to_remove.extend(redeemers);
        } else {
            redeemers.iter().for_each(|x| {
                self.transaction_pool.remove_parent_transaction_id_in_pool(x, transaction_id);
            });
        }

        transactions_to_remove.iter().try_for_each(|x| self.remove_transaction_from_sets(x, remove_redeemers))?;

        if remove_redeemers {
            self.orphan_pool.remove_redeemers_of(transaction_id)?;
        }

        Ok(())
    }

    fn remove_transaction_from_sets(&mut self, transaction_id: &TransactionId, remove_redeemers: bool) -> RuleResult<()> {
        let removed_transaction = self.transaction_pool.remove_transaction(transaction_id)?;
        self.mempool_utxo_set.remove_transaction(&self.transaction_pool, &removed_transaction.mtx);
        self.orphan_pool.update_orphans_after_transaction_removed(&removed_transaction, remove_redeemers)
    }
}
