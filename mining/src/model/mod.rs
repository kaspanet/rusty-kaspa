use kaspa_consensus_core::tx::TransactionId;
use std::collections::HashSet;

pub mod candidate_tx;
pub mod owner_txs;
pub mod topological_index;
pub mod topological_sort;
pub mod tx_insert;
pub mod tx_query;

/// A set of unique transaction ids
pub type TransactionIdSet = HashSet<TransactionId>;
