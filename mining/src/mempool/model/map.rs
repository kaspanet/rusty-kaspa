use ahash::{AHashMap, AHashSet};
use consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};

use super::tx::MempoolTransaction;

/// A set of unique transaction ids
pub(crate) type TransactionIdSet = AHashSet<TransactionId>;

/// IdToTransactionMap maps a transaction id to a mempool transaction
pub(crate) type IdToTransactionMap = AHashMap<TransactionId, MempoolTransaction>;

/// OutpointToIdMap maps an outpoint to a transaction id
pub(crate) type OutpointToIdMap = AHashMap<TransactionOutpoint, TransactionId>;

/// OutpointToUtxoEntryMap maps an outpoint to a UtxoEntry
pub(crate) type OutpointToUtxoEntryMap = AHashMap<TransactionOutpoint, UtxoEntry>;

// /// OutpointToTransactionMap maps an outpoint to a [`MempoolTransaction`]
// pub(crate) type OutpointToTransactionMap = AHashMap<TransactionOutpoint, Rc<MempoolTransaction>>;

// /// ScriptPublicKeyToTransaction maps an outpoint to a [`VerboseTransaction`]
// pub(crate) type ScriptPublicKeyToTransaction = AHashMap<ScriptPublicKey, VerboseTransaction>;

// pub type ScriptPublicKeyToTransaction = AHashMap<ScriptPublicKey, MutableTransaction>;

// pub struct IOScriptToTransaction {
//     sending: ScriptPublicKeyToTransaction,
//     receiving: ScriptPublicKeyToTransaction,
// }
