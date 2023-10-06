use kaspa_consensus_core::tx::TransactionId;
use std::collections::HashSet;

pub(crate) mod candidate_tx;
pub mod owner_txs;
pub mod topological_index;
pub mod topological_sort;

/// A set of unique transaction ids
pub type TransactionIdSet = HashSet<TransactionId>;
