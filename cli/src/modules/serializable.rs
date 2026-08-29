use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_wallet_core::tx::PendingTransaction;
use serde::{Deserialize, Serialize};

/// A serializable wrapper for PendingTransaction
#[derive(Debug, Serialize, Deserialize)]
pub struct SerializablePendingTransaction {
    /// Transaction ID
    pub id: TransactionId,
    /// The raw transaction data
    pub transaction: Transaction,
    /// Fees of the transaction
    pub fees: u64,
    /// Transaction mass
    pub mass: u64,
}

impl From<&PendingTransaction> for SerializablePendingTransaction {
    fn from(tx: &PendingTransaction) -> Self {
        Self {
            id: tx.id(),
            transaction: tx.transaction(),
            fees: tx.fees(),
            mass: tx.mass(),
        }
    }
}

/// A collection of serializable pending transactions
#[derive(Debug, Serialize, Deserialize)]
pub struct SerializablePendingTransactions {
    pub transactions: Vec<SerializablePendingTransaction>,
}

impl SerializablePendingTransactions {
    pub fn from_pending_transactions(txs: &[PendingTransaction]) -> Self {
        Self {
            transactions: txs.iter().map(|tx| tx.into()).collect(),
        }
    }
}
