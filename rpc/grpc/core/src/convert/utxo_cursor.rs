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
        start_covenant_id: item.start_covenant_id.as_ref().map(|id| id.to_string()),
        start_daa_score: item.start_daa_score,
        start_address: item.start_address.as_ref().map(|address| address.to_string()),
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: protowire::RpcGetUtxosByCovenantIdsCursor, kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor, {
    Self {
        start_covenant_id: item.start_covenant_id.as_deref().map(kaspa_rpc_core::RpcHash::from_str).transpose()?,
        start_daa_score: item.start_daa_score,
        start_address: item.start_address.as_deref().map(|address| address.try_into()).transpose()?,
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.try_into()).transpose()?,
    }
});

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::Hash;

    #[test]
    fn test_rpc_get_utxos_by_covenant_ids_cursor_roundtrip() {
        let cursor = kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor {
            start_covenant_id: Some(Hash::from_bytes([3; 32])),
            start_daa_score: 9,
            start_address: None,
            start_outpoint: Some(kaspa_rpc_core::RpcTransactionOutpoint { transaction_id: Hash::from_bytes([1; 32]), index: 2 }),
        };
        let protowire: crate::protowire::RpcGetUtxosByCovenantIdsCursor = (&cursor).into();
        let decoded: kaspa_rpc_core::RpcGetUtxosByCovenantIdsCursor = protowire.try_into().unwrap();
        assert_eq!(decoded, cursor);
    }
}
