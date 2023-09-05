use crate::mempool::{errors::RuleResult, model::pool::Pool, Mempool};
use kaspa_consensus_core::tx::TransactionId;
use kaspa_core::debug;
use kaspa_utils::iter::IterExtensions;

impl Mempool {
    pub(crate) fn remove_transaction(
        &mut self,
        transaction_id: &TransactionId,
        remove_redeemers: bool,
        reason: &str,
        extra_info: &str,
    ) -> RuleResult<()> {
        if self.orphan_pool.has(transaction_id) {
            return self.orphan_pool.remove_orphan(transaction_id, true, reason).map(|_| ());
        }

        if !self.transaction_pool.has(transaction_id) {
            return Ok(());
        }

        let mut removed_transactions = vec![*transaction_id];
        let redeemers = self.transaction_pool.get_redeemer_ids_in_pool(transaction_id);
        if remove_redeemers {
            removed_transactions.extend(redeemers);
        } else {
            redeemers.iter().for_each(|x| {
                self.transaction_pool.remove_parent_chained_relation_in_pool(x, transaction_id);
            });
        }

        removed_transactions.iter().try_for_each(|x| self.remove_transaction_from_sets(x, remove_redeemers))?;

        if remove_redeemers {
            removed_transactions.extend(self.orphan_pool.remove_redeemers_of(transaction_id)?.iter().map(|x| x.id()));
        }

        if !reason.is_empty() {
            match removed_transactions.len() {
                0 => {}
                1 => debug!("Removed transaction ({}) {}{}", reason, removed_transactions[0], extra_info),
                n => debug!(
                    "Removed {} transactions ({}): {}{}",
                    n,
                    reason,
                    removed_transactions.iter().reusable_format(", "),
                    extra_info
                ),
            }
        }

        Ok(())
    }

    fn remove_transaction_from_sets(&mut self, transaction_id: &TransactionId, remove_redeemers: bool) -> RuleResult<()> {
        let removed_transaction = self.transaction_pool.remove_transaction(transaction_id)?;
        self.transaction_pool.remove_transaction_utxos(&removed_transaction.mtx);
        self.orphan_pool.update_orphans_after_transaction_removed(&removed_transaction, remove_redeemers)
    }
}
