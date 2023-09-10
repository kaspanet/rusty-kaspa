use super::TransactionIdSet;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_core::time::Stopwatch;

pub struct TransactionsStagger<T: AsRef<Transaction>> {
    txs: Vec<T>,
    ids: TransactionIdSet,
}

impl<T: AsRef<Transaction>> TransactionsStagger<T> {
    pub fn new(txs: Vec<T>) -> Self {
        let ids = txs.iter().map(|x| x.as_ref().id()).collect();
        Self { txs, ids }
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    /// Extract and return all independent transactions
    pub fn stagger(&mut self) -> Option<Vec<T>> {
        let _sw = Stopwatch::<50>::with_threshold("stagger op");
        if self.is_empty() {
            return None;
        }
        let mut ready = Vec::with_capacity(self.txs.len());
        let mut dependent = Vec::with_capacity(self.txs.len());
        while let Some(tx) = self.txs.pop() {
            if self.is_dependent(&tx) {
                dependent.push(tx);
            } else {
                ready.push(tx);
            }
        }
        self.txs = dependent;
        self.ids = self.txs.iter().map(|x| x.as_ref().id()).collect();
        Some(ready)
    }

    fn has(&self, transaction_id: &TransactionId) -> bool {
        self.ids.contains(transaction_id)
    }

    fn is_dependent(&self, tx: &T) -> bool {
        tx.as_ref().inputs.iter().any(|x| self.has(&x.previous_outpoint.transaction_id))
    }
}
