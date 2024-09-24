use crate::mempool::{
    errors::RuleResult,
    model::{pool::Pool, tx::TxRemovalReason},
    Mempool,
};
use kaspa_consensus_core::tx::TransactionId;
use kaspa_core::debug;
use kaspa_utils::iter::IterExtensions;

impl Mempool {
    pub(crate) fn remove_transaction(
        &mut self,
        transaction_id: &TransactionId,
        remove_redeemers: bool,
        reason: TxRemovalReason,
        extra_info: &str,
    ) -> RuleResult<()> {
        if self.orphan_pool.has(transaction_id) {
            return self.orphan_pool.remove_orphan(transaction_id, true, reason, extra_info).map(|_| ());
        }

        if !self.transaction_pool.has(transaction_id) {
            return Ok(());
        }

        let mut removed_transactions = vec![*transaction_id];
        if remove_redeemers {
            // Add all descendent txs as pending removals
            removed_transactions.extend(self.transaction_pool.get_redeemer_ids_in_pool(transaction_id));
        }

        let mut removed_orphans: Vec<TransactionId> = vec![];
        for tx_id in removed_transactions.iter() {
            // Remove the tx from the transaction pool and the UTXO set (handled within the pool)
            let tx = self.transaction_pool.remove_transaction(tx_id)?;
            // Update/remove descendent orphan txs (depending on `remove_redeemers`)
            let txs = self.orphan_pool.update_orphans_after_transaction_removed(&tx, remove_redeemers)?;
            removed_orphans.extend(txs.into_iter().map(|x| x.id()));
        }
        removed_transactions.extend(removed_orphans);

        match reason {
            TxRemovalReason::Muted => {}
            TxRemovalReason::DoubleSpend => match removed_transactions.len() {
                0 => {}
                1 => debug!("Removed transaction ({}) {}{}", reason, removed_transactions[0], extra_info),
                n => debug!(
                    "Removed {} transactions ({}): {}{}",
                    n,
                    reason,
                    removed_transactions.iter().reusable_format(", "),
                    extra_info
                ),
            },
            _ => match removed_transactions.len() {
                0 => {}
                1 => debug!("Removed transaction ({}) {}{}", reason, removed_transactions[0], extra_info),
                n => debug!(
                    "Removed {} transactions ({}): {}{}",
                    n,
                    reason,
                    removed_transactions.iter().reusable_format(", "),
                    extra_info
                ),
            },
        }

        Ok(())
    }
}
