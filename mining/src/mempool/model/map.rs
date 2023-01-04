use super::tx::MempoolTransaction;
use consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::{HashMap, HashSet};

/// A set of unique transaction ids
pub(crate) type TransactionIdSet = HashSet<TransactionId>;

/// IdToTransactionMap maps a transaction id to a mempool transaction
pub(crate) type IdToTransactionMap = HashMap<TransactionId, MempoolTransaction>;

/// OutpointToIdMap maps an outpoint to a transaction id
pub(crate) type OutpointToIdMap = HashMap<TransactionOutpoint, TransactionId>;
