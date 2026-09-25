use crate::protowire::{self};
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcGetUtxosByAddressesCursor, protowire::RpcGetUtxosByAddressesCursor, {
    Self {
        start_address: item.start_address.to_string(),
        start_daa_score: item.start_daa_score,
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: protowire::RpcGetUtxosByAddressesCursor, kaspa_rpc_core::RpcGetUtxosByAddressesCursor, {
    Self {
        start_address: item.start_address.try_into()?,
        start_daa_score: item.start_daa_score,
        start_outpoint: item.start_outpoint.as_ref().map(|o| o.try_into()).transpose()?,
    }
});
