use consensus_core::tx::TransactionId;
use std::collections::HashSet;

pub(crate) mod candidate_tx;
pub mod owner_txs;
pub mod topological_index;

/// A set of unique transaction ids
pub(crate) type TransactionIdSet = HashSet<TransactionId>;
