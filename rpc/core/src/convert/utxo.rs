//! Conversion functions for UTXO related types.

use crate::RpcUtxoEntry;
use crate::RpcUtxosByAddressesEntry;
use kaspa_addresses::Prefix;
use kaspa_index_core::indexed_utxos::OrderedUtxoEntries;
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
                .map(|(utxo_key_suffix_record, entry)| RpcUtxosByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*utxo_key_suffix_record.transaction_outpoint()).into(),
                    utxo_entry: RpcUtxoEntry::new(
                        entry.amount,
                        script_public_key.clone(),
                        utxo_key_suffix_record.daa_score(),
                        entry.is_coinbase,
                        entry.covenant_id,
                    ),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn ordered_utxo_set_into_rpc(item: &OrderedUtxoEntries, prefix: Option<Prefix>) -> Vec<RpcUtxosByAddressesEntry> {
    item.iter()
        .flat_map(|(script_public_key, utxo_collection)| {
            let address = prefix.and_then(|x| extract_script_pub_key_address(script_public_key, x).ok());
            utxo_collection
                .iter()
                .map(|(utxo_key_suffix_record, entry)| RpcUtxosByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*utxo_key_suffix_record.transaction_outpoint()).into(),
                    utxo_entry: RpcUtxoEntry::new(
                        entry.amount,
                        script_public_key.clone(),
                        utxo_key_suffix_record.daa_score(),
                        entry.is_coinbase,
                        entry.covenant_id,
                    ),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}
