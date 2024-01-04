use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
// One possible implementation: u64 of transaction id xor'd with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<TransactionOutpoint, CompactUtxoEntry>;

/// A collection of utxos indexed via; [`ScriptPublicKey`] => [`TransactionOutpoint`] => [`CompactUtxoEntry`].
pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

/// A map of balance by script public key
pub type BalanceByScriptPublicKey = HashMap<ScriptPublicKey, u64>;

// Note: memory optimization compared to go-lang kaspad:
// Unlike `consensus_core::tx::UtxoEntry` the utxoindex utilizes a compacted utxo form, where `script_public_key` field is removed.
// This utxo structure can be utilized in the utxoindex, since utxos are implicitly key'd via its script public key (and outpoint) at all times.
/// A compacted form of [`UtxoEntry`] without reference to [`ScriptPublicKey`] or [`TransactionOutpoint`]
#[derive(Clone, Copy, Deserialize, Serialize, Debug)]
pub struct CompactUtxoEntry {
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl CompactUtxoEntry {
    /// Creates a new [`CompactUtxoEntry`]
    pub fn new(amount: u64, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, block_daa_score, is_coinbase }
    }
}

impl MemSizeEstimator for CompactUtxoEntry {}

impl From<UtxoEntry> for CompactUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self { amount: utxo_entry.amount, block_daa_score: utxo_entry.block_daa_score, is_coinbase: utxo_entry.is_coinbase }
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
