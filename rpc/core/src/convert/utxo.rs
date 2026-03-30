//! Conversion functions for UTXO related types.

use crate::RpcUtxoEntry;
use crate::RpcUtxosByAddressesEntry;
use kaspa_addresses::Prefix;
use kaspa_index_core::indexed_utxos::UtxoSetByScriptPublicKey;
use kaspa_txscript::extract_script_pub_key_address;

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

pub fn utxo_set_into_rpc(item: &UtxoSetByScriptPublicKey, prefix: Option<Prefix>) -> Vec<RpcUtxosByAddressesEntry> {
    item.iter()
        .flat_map(|(script_public_key, utxo_collection)| {
            let address = prefix.and_then(|x| extract_script_pub_key_address(script_public_key, x).ok());
            utxo_collection
                .iter()
                .map(|(outpoint, entry)| RpcUtxosByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*outpoint).into(),
                    utxo_entry: RpcUtxoEntry::new(entry.amount, script_public_key.clone(), entry.block_daa_score, entry.is_coinbase),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn utxo_set_into_rpc_after_daa_score(
    item: &UtxoSetByScriptPublicKey,
    prefix: Option<Prefix>,
    start_daa_score: u64,
) -> Vec<RpcUtxosByAddressesEntry> {
    item.iter()
        .flat_map(|(script_public_key, utxo_collection)| {
            let address = prefix.and_then(|x| extract_script_pub_key_address(script_public_key, x).ok());
            utxo_collection
                .iter()
                .filter(|(_, entry)| entry.block_daa_score > start_daa_score)
                .map(|(outpoint, entry)| RpcUtxosByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*outpoint).into(),
                    utxo_entry: RpcUtxoEntry::new(entry.amount, script_public_key.clone(), entry.block_daa_score, entry.is_coinbase),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
    use kaspa_hashes::Hash;
    use kaspa_index_core::indexed_utxos::{CompactUtxoCollection, CompactUtxoEntry};
    use std::collections::HashMap;

    #[test]
    fn utxo_set_into_rpc_after_daa_score_filters_strictly_greater() {
        let script_public_key = ScriptPublicKey::new(0, vec![1, 2, 3].into());
        let mut utxos = CompactUtxoCollection::new();

        utxos.insert(
            TransactionOutpoint { transaction_id: Hash::from_bytes([1u8; 32]), index: 0 },
            CompactUtxoEntry::new(100, 10, false),
        );
        utxos.insert(
            TransactionOutpoint { transaction_id: Hash::from_bytes([2u8; 32]), index: 1 },
            CompactUtxoEntry::new(200, 11, false),
        );

        let mut set: UtxoSetByScriptPublicKey = HashMap::new();
        set.insert(script_public_key.clone(), utxos);

        let filtered = utxo_set_into_rpc_after_daa_score(&set, None, 10);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].utxo_entry.amount, 200);
        assert_eq!(filtered[0].utxo_entry.block_daa_score, 11);
        assert_eq!(filtered[0].utxo_entry.script_public_key, script_public_key);
    }
}
