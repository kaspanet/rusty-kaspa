use crate::mempool::{
    errors::RuleResult,
    model::{
        pool::Pool,
        tx::{MempoolTransaction, TxRemovalReason},
    },
    Mempool,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    tx::{Transaction, TransactionId},
};
use kaspa_core::time::Stopwatch;
use std::{collections::HashSet, sync::atomic::Ordering};

impl Mempool {
    pub(crate) fn handle_new_block_transactions(
        &mut self,
        block_daa_score: u64,
        block_transactions: &[Transaction],
    ) -> RuleResult<Vec<MempoolTransaction>> {
        let _sw = Stopwatch::<400>::with_threshold("handle_new_block_transactions op");
        let mut unorphaned_transactions = vec![];
        for transaction in block_transactions[1..].iter() {
            let transaction_id = transaction.id();
            // Rust rewrite: This behavior does differ from golang implementation.
            // If the transaction got accepted via a peer but is still an orphan here, do not remove
            // its redeemers in the orphan pool. We give those a chance to be unorphaned and included
            // in the next block template.
            if !self.orphan_pool.has(&transaction_id) {
                self.remove_transaction(&transaction_id, false, TxRemovalReason::Accepted, "")?;
            }
            self.remove_double_spends(transaction)?;
            self.orphan_pool.remove_orphan(&transaction_id, false, TxRemovalReason::Accepted, "")?;
            self.counters.block_tx_counts.fetch_add(1, Ordering::SeqCst);
            if self.accepted_transactions.add(transaction_id, block_daa_score) {
                self.counters.tx_accepted_counts.fetch_add(1, Ordering::SeqCst);
                self.counters.input_counts.fetch_add(transaction.inputs.len() as u64, Ordering::SeqCst);
                self.counters.output_counts.fetch_add(transaction.outputs.len() as u64, Ordering::SeqCst);
            }
            unorphaned_transactions.extend(self.get_unorphaned_transactions_after_accepted_transaction(transaction));
        }

        // Update the sample of number of ready transactions in the mempool and log the stats
        self.counters.ready_txs_sample.store(self.transaction_pool.ready_transaction_count() as u64, Ordering::SeqCst);
        self.log_stats();

        Ok(unorphaned_transactions)
    }

    pub(crate) fn expire_orphan_low_priority_transactions(&mut self, consensus: &dyn ConsensusApi) -> RuleResult<()> {
        self.orphan_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())
    }

    pub(crate) fn expire_accepted_transactions(&mut self, consensus: &dyn ConsensusApi) {
        self.accepted_transactions.expire(consensus.get_virtual_daa_score());
    }

    pub(crate) fn collect_expired_low_priority_transactions(&mut self, consensus: &dyn ConsensusApi) -> Vec<TransactionId> {
        self.transaction_pool.collect_expired_low_priority_transactions(consensus.get_virtual_daa_score())
    }

    fn remove_double_spends(&mut self, transaction: &Transaction) -> RuleResult<()> {
        let mut transactions_to_remove = HashSet::new();
        for input in transaction.inputs.iter() {
            if let Some(redeemer_id) = self.transaction_pool.get_outpoint_owner_id(&input.previous_outpoint) {
                transactions_to_remove.insert(*redeemer_id);
            }
        }
        transactions_to_remove.iter().try_for_each(|x| {
            self.remove_transaction(x, true, TxRemovalReason::DoubleSpend, format!(" favouring {}", transaction.id()).as_str())
        })
    }
}
