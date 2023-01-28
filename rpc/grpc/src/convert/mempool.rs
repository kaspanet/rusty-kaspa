use crate::protowire;
use crate::{from, try_from};
use rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &rpc_core::RpcMempoolEntry, protowire::RpcMempoolEntry, {
    Self { fee: item.fee, transaction: Some((&item.transaction).into()), is_orphan: item.is_orphan }
});

from!(item: &rpc_core::RpcMempoolEntryByAddress, protowire::RpcMempoolEntryByAddress, {
    Self {
        address: (&item.address).into(),
        sending: item.sending.iter().map(|x| x.into()).collect::<Vec<_>>(),
        receiving: item.receiving.iter().map(|x| x.into()).collect::<Vec<_>>(),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcMempoolEntry, rpc_core::RpcMempoolEntry, {
    Self::new(
        item.fee,
        item.transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcMempoolEntry".to_string(), "transaction".to_string()))?
            .try_into()?,
        item.is_orphan,
    )
});

try_from!(item: &protowire::RpcMempoolEntryByAddress, rpc_core::RpcMempoolEntryByAddress, {
    Self::new(
        item.address.as_str().try_into()?,
        item.sending.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
        item.receiving.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    )
});
