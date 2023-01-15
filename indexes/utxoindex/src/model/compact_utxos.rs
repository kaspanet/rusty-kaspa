use std::collections::HashMap;

use consensus_core::tx::TransactionOutpoint;
//TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
//One possible implementation: u64 of transaction id xored with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<TransactionOutpoint, CompactUtxoEntry>;


use consensus_core::tx::UtxoEntry;
use serde::{Deserialize, Serialize};

//Note: memory optimizaion compared to go-lang kaspad:
//unlike `consensus_core::tx::UtxoEntry` the utxoindex utilizes a comapacted utxo form, where `script_public_key` field is removed.
//this utxo structure can be utilized since utxos are implicitly key'd via its script public key (and outpoint) at all times. 
#[derive(Clone, Deserialize, Serialize)]
pub struct CompactUtxoEntry {
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl From<UtxoEntry> for CompactUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self { amount: utxo_entry.amount, block_daa_score: utxo_entry.block_daa_score, is_coinbase: utxo_entry.is_coinbase }
    }
}
