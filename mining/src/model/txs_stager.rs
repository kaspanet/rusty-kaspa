use super::TransactionIdSet;
use kaspa_consensus_core::tx::{Transaction, TransactionId};

pub struct TransactionsStagger {
    txs: Vec<Transaction>,
    ids: TransactionIdSet,
}

impl TransactionsStagger {
    pub fn new(txs: Vec<Transaction>) -> Self {
        let ids = txs.iter().map(|x| x.id()).collect();
        Self { txs, ids }
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    /// Extract and return all independent transactions
    pub fn stagger(&mut self) -> Option<Vec<Transaction>> {
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
        self.ids = self.txs.iter().map(|x| x.id()).collect();
        (!self.is_empty()).then_some(ready)
    }

    pub fn has(&self, transaction_id: &TransactionId) -> bool {
        self.ids.contains(transaction_id)
    }

    pub fn is_dependent(&self, tx: &Transaction) -> bool {
        tx.inputs.iter().any(|x| self.has(&x.previous_outpoint.transaction_id))
    }
}
