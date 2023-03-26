use crate::pb as protowire;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&UtxoEntry> for protowire::UtxoEntry {
    fn from(entry: &UtxoEntry) -> Self {
        Self {
            amount: entry.amount,
            script_public_key: Some((&entry.script_public_key).into()),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}

impl From<(&TransactionOutpoint, &UtxoEntry)> for protowire::OutpointAndUtxoEntryPair {
    fn from((outpoint, entry): (&TransactionOutpoint, &UtxoEntry)) -> Self {
        Self { outpoint: Some(outpoint.into()), utxo_entry: Some(entry.into()) }
    }
}
