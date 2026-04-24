use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcResult};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcOptionalBlock, protowire::RpcOptionalBlock, {
    Self {
        header: item.header.as_ref().map(|h| h.into()),
        transactions: item.transactions.iter().map(protowire::RpcOptionalTransaction::from).collect(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcOptionalBlock, kaspa_rpc_core::RpcOptionalBlock, {
    Self {
        header: item.header.as_ref().map(|h| h.try_into()).transpose()?,
        transactions: item.transactions.iter()
            .map(kaspa_rpc_core::RpcOptionalTransaction::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
        verbose_data: item.verbose_data.as_ref()
            .map(kaspa_rpc_core::RpcBlockVerboseData::try_from).transpose()?,
    }
});
