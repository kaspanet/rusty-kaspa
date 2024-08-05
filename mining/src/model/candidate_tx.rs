use crate::FeerateTransactionKey;
use kaspa_consensus_core::tx::Transaction;
use std::sync::Arc;

/// Transaction with additional metadata needed in order to be a candidate
/// in the transaction selection algorithm
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateTransaction {
    /// The actual transaction
    pub tx: Arc<Transaction>,
    /// Populated fee
    pub calculated_fee: u64,
    /// Populated mass
    pub calculated_mass: u64,
}

impl CandidateTransaction {
    pub fn from_key(key: FeerateTransactionKey) -> Self {
        Self { tx: key.tx, calculated_fee: key.fee, calculated_mass: key.mass }
    }
}
