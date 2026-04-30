use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
// One possible implementation: u64 of transaction id xor'd with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<UtxoEntryKeyData, CompactUtxoEntry>;

/// A deterministic ordered list of UTXOs keyed by [`UtxoEntryKeyData`].
pub type OrderedUtxoCollection = Vec<(UtxoEntryKeyData, CompactUtxoEntry)>;

/// A deterministic ordered list of UTXO collections keyed by [`ScriptPublicKey`].
pub type OrderedUtxoSetByScriptPublicKey = Vec<(ScriptPublicKey, OrderedUtxoCollection)>;

/// A collection of utxos indexed via; [`ScriptPublicKey`] => [`TransactionOutpoint`] => [`CompactUtxoEntry`].
pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

/// A map of balance by script public key
pub type BalanceByScriptPublicKey = HashMap<ScriptPublicKey, u64>;

// Note: memory optimization compared to go-lang kaspad:
// Unlike `consensus_core::tx::UtxoEntry` the utxoindex utilizes a compacted utxo form, where `script_public_key` field is removed.
// This utxo structure can be utilized in the utxoindex, since utxos are implicitly key'd via its script public key (and outpoint) at all times.
// furthermore, the daa_score is also added to the key (for range queries) and thus is removed from the value as well. This results in a more compact representation of the utxo entry, which is more suitable for storage in the utxoindex.
/// A compacted form of [`UtxoEntry`] without reference to [`ScriptPublicKey`] or [`TransactionOutpoint`]
#[derive(Clone, Copy, Deserialize, Serialize, Debug)]
pub struct CompactUtxoEntry {
    pub amount: u64,
    pub is_coinbase: bool,
}

impl CompactUtxoEntry {
    /// Creates a new [`CompactUtxoEntry`]
    pub fn new(amount: u64, is_coinbase: bool) -> Self {
        Self { amount, is_coinbase }
    }
}

impl MemSizeEstimator for CompactUtxoEntry {}

impl From<UtxoEntry> for CompactUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self { amount: utxo_entry.amount, is_coinbase: utxo_entry.is_coinbase }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct UtxoEntryKeyData {
    pub daa_score: u64,
    pub transaction_outpoint: TransactionOutpoint,
}

impl UtxoEntryKeyData {
    pub fn new(daa_score: u64, transaction_outpoint: TransactionOutpoint) -> Self {
        Self { daa_score, transaction_outpoint }
    }
}

/// A struct holding utxo changes to the utxoindex via `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
#[derive(Debug, Clone)]
pub struct UtxoChanges {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UtxoChanges {
    /// Create a new [`UtxoChanges`] struct via supplied `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
    pub fn new(added: UtxoSetByScriptPublicKey, removed: UtxoSetByScriptPublicKey) -> Self {
        Self { added, removed }
    }
}
