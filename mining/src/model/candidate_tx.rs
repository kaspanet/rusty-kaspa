use kaspa_consensus_core::tx::{MutableTransaction, Transaction};
use std::sync::Arc;

/// Transaction with additional metadata needed in order to be a candidate
/// in the transaction selection algorithm
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateTransaction {
    /// The actual transaction
    pub tx: Arc<Transaction>,
    /// Populated fee
    pub calculated_fee: u64,
    /// Populated mass
    pub calculated_mass: u64,
}

impl CandidateTransaction {
    pub(crate) fn from_mutable(tx: &MutableTransaction) -> Self {
        Self {
            tx: tx.tx.clone(),
            calculated_fee: tx.calculated_fee.expect("fee is expected to be populated"),
            calculated_mass: tx.calculated_mass.expect("mass is expected to be populated"),
        }
    }
}
