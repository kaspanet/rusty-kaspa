use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};

//TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
//One possible implementation: u64 of transaction id xored with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<TransactionOutpoint, CompactUtxoEntry>;

/// A collection of utxos indexed via; [`ScriptPublicKey`] => [`TransactionOutpoint`] => [`CompactUtxo`].
pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

//Note: memory optimizaion compared to go-lang kaspad:
//unlike `consensus_core::tx::UtxoEntry` the utxoindex utilizes a comapacted utxo form, where `script_public_key` field is removed.
//this utxo structure can be utilized since utxos are implicitly key'd via its script public key (and outpoint) at all times.
///A compacted form of [`UtxoEntry`] without reference to [`ScriptPublicKey`] or [`TransactionOutpoint`]
#[derive(Clone, Copy, Deserialize, Serialize, Debug)]
pub struct CompactUtxoEntry {
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}
impl CompactUtxoEntry {
    ///create a new [`CompactUtxoEntry`]
    pub fn new(amount: u64, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, block_daa_score, is_coinbase }
    }
}
impl From<UtxoEntry> for CompactUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self { amount: utxo_entry.amount, block_daa_score: utxo_entry.block_daa_score, is_coinbase: utxo_entry.is_coinbase }
    }
}

///A struct holding UTXO changes to the utxoindex via `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
#[derive(Debug, Clone)]
pub struct UTXOChanges {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UTXOChanges {
    ///create a new [`UtxoChanges`] struct via supplied `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
    pub fn new(added: UtxoSetByScriptPublicKey, removed: UtxoSetByScriptPublicKey) -> Self {
        Self { added, removed }
    }
}
