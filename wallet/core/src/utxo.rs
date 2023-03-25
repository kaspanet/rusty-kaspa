use std::sync::Arc;

use consensus_core::tx::UtxoEntry;

pub struct AccountUtxoEntry {
    pub utxo_entry: Arc<UtxoEntry>,
}

pub struct AccountUtxoEntryList {
    pub utxo_entryies: Vec<AccountUtxoEntry>,
}
