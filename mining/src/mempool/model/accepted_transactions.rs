use crate::mempool::config::Config;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_core::{debug, time::unix_now};
use std::{collections::HashMap, sync::Arc};

pub(crate) struct AcceptedTransactions {
    /// Mempool config
    config: Arc<Config>,

    /// A map of Transaction IDs to DAA scores
    transactions: HashMap<TransactionId, u64>,

    /// Last expire scan DAA score
    last_expire_scan_daa_score: u64,
    /// last expire scan time in milliseconds
    last_expire_scan_time: u64,
}

impl AcceptedTransactions {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self { config, transactions: Default::default(), last_expire_scan_daa_score: 0, last_expire_scan_time: unix_now() }
    }

    pub(crate) fn add(&mut self, transaction_id: TransactionId, daa_score: u64) -> bool {
        self.transactions.insert(transaction_id, daa_score).is_none()
    }

    pub(crate) fn remove(&mut self, transaction_id: &TransactionId) -> bool {
        self.transactions.remove(transaction_id).is_some()
    }

    pub(crate) fn has(&self, transaction_id: &TransactionId) -> bool {
        self.transactions.contains_key(transaction_id)
    }

    pub(crate) fn len(&self) -> usize {
        self.transactions.len()
    }

    pub(crate) fn unaccepted(&self, transactions: &mut impl Iterator<Item = TransactionId>) -> Vec<TransactionId> {
        transactions.filter(|transaction_id| !self.has(transaction_id)).collect()
    }

    pub(crate) fn expire(&mut self, virtual_daa_score: u64) {
        let now = unix_now();
        if virtual_daa_score
            < self.last_expire_scan_daa_score + self.config.accepted_transaction_expire_scan_interval_daa_score.after()
            || now < self.last_expire_scan_time + self.config.accepted_transaction_expire_scan_interval_milliseconds
        {
            return;
        }

        let expired_transactions: Vec<TransactionId> = self
            .transactions
            .iter()
            .filter_map(|(transaction_id, daa_score)| {
                if virtual_daa_score > daa_score + self.config.accepted_transaction_expire_interval_daa_score.after() {
                    Some(*transaction_id)
                } else {
                    None
                }
            })
            .collect();

        for transaction_id in expired_transactions.iter() {
            self.remove(transaction_id);
        }

        debug!(
            "Removed {} accepted transactions from mempool cache. Currently containing {}",
            expired_transactions.len(),
            self.transactions.len()
        );

        self.last_expire_scan_daa_score = virtual_daa_score;
        self.last_expire_scan_time = now;
    }
}
