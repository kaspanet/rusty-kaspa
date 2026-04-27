//! Conversion functions for UTXO related types.

use crate::{RpcUtxoEntry, RpcUtxoReferenceEntry, RpcUtxosByAddressesEntry};
use kaspa_addresses::Prefix;
use kaspa_index_core::indexed_utxos::{UtxoReferenceEntry, UtxoSetByScriptPublicKey};
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
                    utxo_entry: RpcUtxoEntry::new(
                        entry.amount,
                        script_public_key.clone(),
                        entry.block_daa_score,
                        entry.is_coinbase,
                        entry.covenant_id,
                    ),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn utxo_references_into_rpc(item: &[UtxoReferenceEntry]) -> Vec<RpcUtxoReferenceEntry> {
    item.iter()
        .map(|entry| RpcUtxoReferenceEntry { outpoint: entry.outpoint.into(), utxo_entry: entry.utxo_entry.clone().into() })
        .collect()
}
