use crate::protowire;
use rpc_core::{RpcError, RpcHash, RpcResult};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::GetBlockRequest> for protowire::GetBlockRequestMessage {
    fn from(item: &rpc_core::GetBlockRequest) -> Self {
        Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
    }
}

impl From<RpcResult<&rpc_core::GetBlockResponse>> for protowire::GetBlockResponseMessage {
    fn from(item: RpcResult<&rpc_core::GetBlockResponse>) -> Self {
        Self {
            block: item.as_ref().map(|x| protowire::RpcBlock::from(&x.block)).ok(),
            error: item.map_err(protowire::RpcError::from).err(),
        }
    }
}

impl From<&rpc_core::NotifyBlockAddedRequest> for protowire::NotifyBlockAddedRequestMessage {
    fn from(item: &rpc_core::NotifyBlockAddedRequest) -> Self {
        Self { command: item.command.into() }
    }
}

impl From<RpcResult<&rpc_core::NotifyBlockAddedResponse>> for protowire::NotifyBlockAddedResponseMessage {
    fn from(item: RpcResult<&rpc_core::NotifyBlockAddedResponse>) -> Self {
        Self { error: item.map_err(protowire::RpcError::from).err() }
    }
}

impl From<&rpc_core::GetInfoRequest> for protowire::GetInfoRequestMessage {
    fn from(_item: &rpc_core::GetInfoRequest) -> Self {
        Self {}
    }
}

impl From<RpcResult<&rpc_core::GetInfoResponse>> for protowire::GetInfoResponseMessage {
    fn from(item: RpcResult<&rpc_core::GetInfoResponse>) -> Self {
        match item {
            Ok(response) => Self {
                p2p_id: response.p2p_id.clone(),
                mempool_size: response.mempool_size,
                server_version: response.server_version.clone(),
                is_utxo_indexed: response.is_utxo_indexed,
                is_synced: response.is_synced,
                has_notify_command: response.has_notify_command,
                error: None,
            },
            Err(err) => Self {
                p2p_id: String::default(),
                mempool_size: 0,
                server_version: String::default(),
                is_utxo_indexed: false,
                is_synced: false,
                has_notify_command: false,
                error: Some(err.into()),
            },
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::GetBlockRequestMessage> for rpc_core::GetBlockRequest {
    type Error = RpcError;
    fn try_from(item: &protowire::GetBlockRequestMessage) -> RpcResult<Self> {
        Ok(Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions })
    }
}

impl TryFrom<&protowire::GetBlockResponseMessage> for rpc_core::GetBlockResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::GetBlockResponseMessage) -> RpcResult<Self> {
        item.block
            .as_ref()
            .map_or_else(
                || {
                    item.error
                        .as_ref()
                        .map_or(Err(RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "error".to_string())), |x| {
                            Err(x.into())
                        })
                },
                rpc_core::RpcBlock::try_from,
            )
            .map(|x| rpc_core::GetBlockResponse { block: x })
    }
}

impl TryFrom<&protowire::NotifyBlockAddedRequestMessage> for rpc_core::NotifyBlockAddedRequest {
    type Error = RpcError;
    fn try_from(item: &protowire::NotifyBlockAddedRequestMessage) -> RpcResult<Self> {
        Ok(Self { command: item.command.into() })
    }
}

impl TryFrom<&protowire::NotifyBlockAddedResponseMessage> for rpc_core::NotifyBlockAddedResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::NotifyBlockAddedResponseMessage) -> RpcResult<Self> {
        item.error.as_ref().map_or(Ok(rpc_core::NotifyBlockAddedResponse {}), |x| Err(x.into()))
    }
}

impl TryFrom<&protowire::GetInfoRequestMessage> for rpc_core::GetInfoRequest {
    type Error = RpcError;
    fn try_from(_item: &protowire::GetInfoRequestMessage) -> RpcResult<Self> {
        Ok(Self {})
    }
}

impl TryFrom<&protowire::GetInfoResponseMessage> for rpc_core::GetInfoResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::GetInfoResponseMessage) -> RpcResult<Self> {
        if let Some(err) = item.error.as_ref() {
            Err(err.into())
        } else {
            Ok(Self {
                p2p_id: item.p2p_id.clone(),
                mempool_size: item.mempool_size,
                server_version: item.server_version.clone(),
                is_utxo_indexed: item.is_utxo_indexed,
                is_synced: item.is_synced,
                has_notify_command: item.has_notify_command,
            })
        }
    }
}
