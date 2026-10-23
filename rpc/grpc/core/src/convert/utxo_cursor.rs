use crate::protowire::{self};
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcGetUtxosByAddressesCursor, protowire::RpcGetUtxosByAddressesCursor, {
    Self {
        start_address: item.start_address.as_ref().map(|address| address.to_string()),
        start_daa_score: item.start_daa_score,
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: protowire::RpcGetUtxosByAddressesCursor, kaspa_rpc_core::RpcGetUtxosByAddressesCursor, {
    Self {
        start_address: item.start_address.as_deref().map(|address| address.try_into()).transpose()?,
        start_daa_score: item.start_daa_score,
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.try_into()).transpose()?,
    }
});

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor, protowire::RpcGetUtxosByCovenantIdsCursor, {
    Self {
        start_covenant_id: item.start_covenant_id.to_string(),
        start_daa_score: item.start_daa_score,
        start_script_public_key: Some((&item.start_script_public_key).into()),
        start_outpoint: Some((&item.start_outpoint).into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: protowire::RpcGetUtxosByCovenantIdsCursor, kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor, {
    Self {
        start_covenant_id: kaspa_rpc_core::RpcHash::from_str(&item.start_covenant_id)?,
        start_daa_score: item.start_daa_score,
        start_script_public_key: item.start_script_public_key.as_ref().ok_or_else(missing_start_script_public_key)?.try_into()?,
        start_outpoint: item.start_outpoint.as_ref().ok_or_else(missing_start_outpoint)?.try_into()?,
    }
});

fn missing_start_script_public_key() -> RpcError {
    RpcError::MissingRpcFieldError("RpcGetUtxosByCovenantIdsCursor".to_string(), "startScriptPublicKey".to_string())
}

fn missing_start_outpoint() -> RpcError {
    RpcError::MissingRpcFieldError("RpcGetUtxosByCovenantIdsCursor".to_string(), "startOutpoint".to_string())
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::Hash;

    #[test]
    fn test_rpc_get_utxos_by_covenant_ids_cursor_roundtrip() {
        let cursor = kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor {
            start_covenant_id: Hash::from_bytes([3; 32]),
            start_daa_score: 9,
            start_script_public_key: kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x07]),
            start_outpoint: kaspa_rpc_core::RpcTransactionOutpoint { transaction_id: Hash::from_bytes([1; 32]), index: 2 },
        };
        let protowire: crate::protowire::RpcGetUtxosByCovenantIdsCursor = (&cursor).into();
        let decoded: kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor = protowire.try_into().unwrap();
        assert_eq!(decoded, cursor);
    }
}
