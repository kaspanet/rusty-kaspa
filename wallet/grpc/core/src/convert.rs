use crate::kaspawalletd::{Outpoint, ScriptPublicKey, UtxoEntry, UtxosByAddressesEntry};
use kaspa_wallet_core::api::{ScriptPublicKeyWrapper, TransactionOutpointWrapper, UtxoEntryWrapper};

impl From<TransactionOutpointWrapper> for Outpoint {
    fn from(wrapper: kaspa_wallet_core::api::TransactionOutpointWrapper) -> Self {
        Outpoint { transaction_id: wrapper.transaction_id.to_string(), index: wrapper.index }
    }
}

impl From<ScriptPublicKeyWrapper> for ScriptPublicKey {
    fn from(script_pub_key: ScriptPublicKeyWrapper) -> Self {
        ScriptPublicKey { script_public_key: script_pub_key.script_public_key, version: script_pub_key.version.into() }
    }
}

impl From<UtxoEntryWrapper> for UtxosByAddressesEntry {
    fn from(wrapper: UtxoEntryWrapper) -> Self {
        UtxosByAddressesEntry {
            address: wrapper.address.map(|addr| addr.to_string()).unwrap_or_default(),
            outpoint: Some(wrapper.outpoint.into()),
            utxo_entry: Some(UtxoEntry {
                amount: wrapper.amount,
                script_public_key: Some(wrapper.script_public_key.into()),
                block_daa_score: wrapper.block_daa_score,
                is_coinbase: wrapper.is_coinbase,
            }),
        }
    }
}
