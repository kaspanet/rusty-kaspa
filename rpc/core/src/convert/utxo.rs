use crate::RpcUtxosByAddressesEntry;
use kaspa_index_core::indexed_utxos::UtxoSetByScriptPublicKey;

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

pub fn utxo_set_into_rpc(_item: &UtxoSetByScriptPublicKey) -> Vec<RpcUtxosByAddressesEntry> {
    // TODO: handle address/script_public_key pairs
    //       this will be possible when txscript will have golang PayToAddrScript and ExtractScriptPubKeyAddress ported

    let result = vec![];
    // for (script, utxo_set) in item {
    //     result.extend(utxo_set.iter().map(|(outpoint, entry)|
    //     RpcUtxosByAddressesEntry {
    //         address: ,
    //         outpoint: outpoint.clone(),
    //         utxo_entry: UtxoEntry::new(entry.amount, script.clone(), entry.block_daa_score, entry.is_coinbase),
    //     }
    //     ));
    // }
    result
}
