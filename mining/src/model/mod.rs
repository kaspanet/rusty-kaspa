use consensus_core::tx::TransactionId;
use std::collections::HashSet;

pub mod owner_txs;
pub mod topological_index;

/// A set of unique transaction ids
pub(crate) type TransactionIdSet = HashSet<TransactionId>;
