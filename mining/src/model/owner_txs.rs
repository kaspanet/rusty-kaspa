use consensus_core::tx::{MutableTransaction, ScriptPublicKey, TransactionId};
use std::collections::{HashMap, HashSet};

use super::TransactionIdSet;

pub type ScriptPublicKeySet = HashSet<ScriptPublicKey>;

/// Transaction ids involved in either sending to or receiving from an
/// address or its [`ScriptPublicKey`] equivalent.
#[derive(Default)]
pub struct OwnerTransactions {
    pub sending_txs: TransactionIdSet,
    pub receiving_txs: TransactionIdSet,
}

impl OwnerTransactions {
    pub fn is_empty(&self) -> bool {
        self.sending_txs.is_empty() && self.receiving_txs.is_empty()
    }
}

#[derive(Default)]
pub struct OwnerSetTransactions {
    pub transactions: HashMap<TransactionId, MutableTransaction>,
    pub owners: HashMap<ScriptPublicKey, OwnerTransactions>,
}
