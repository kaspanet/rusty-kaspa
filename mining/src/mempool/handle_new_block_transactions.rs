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
        let mut tx_accepted_counts = 0;
        let mut input_counts = 0;
        let mut output_counts = 0;
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
            if self.accepted_transactions.add(transaction_id, block_daa_score) {
                tx_accepted_counts += 1;
                input_counts += transaction.inputs.len();
                output_counts += transaction.outputs.len();
            }
            unorphaned_transactions.extend(self.get_unorphaned_transactions_after_accepted_transaction(transaction));
        }
        self.counters.block_tx_counts.fetch_add(block_transactions.len() as u64 - 1, Ordering::Relaxed);
        self.counters.tx_accepted_counts.fetch_add(tx_accepted_counts, Ordering::Relaxed);
        self.counters.input_counts.fetch_add(input_counts as u64, Ordering::Relaxed);
        self.counters.output_counts.fetch_add(output_counts as u64, Ordering::Relaxed);
        self.counters.ready_txs_sample.store(self.transaction_pool.ready_transaction_count() as u64, Ordering::Relaxed);
        self.counters.txs_sample.store(self.transaction_pool.len() as u64, Ordering::Relaxed);
        self.counters.orphans_sample.store(self.orphan_pool.len() as u64, Ordering::Relaxed);
        self.counters.accepted_sample.store(self.accepted_transactions.len() as u64, Ordering::Relaxed);

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
