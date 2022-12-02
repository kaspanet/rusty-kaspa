use crate::protowire::{self, submit_block_response_message::RejectReason};
use rpc_core::{RpcError, RpcExtraData, RpcHash, RpcResult};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::SubmitBlockRequest> for protowire::SubmitBlockRequestMessage {
    fn from(item: &rpc_core::SubmitBlockRequest) -> Self {
        Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
    }
}

impl From<&rpc_core::SubmitBlockReport> for RejectReason {
    fn from(item: &rpc_core::SubmitBlockReport) -> Self {
        match item {
            rpc_core::SubmitBlockReport::Success => RejectReason::None,
            rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
            rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
        }
    }
}

impl From<RpcResult<&rpc_core::SubmitBlockResponse>> for protowire::SubmitBlockResponseMessage {
    fn from(item: RpcResult<&rpc_core::SubmitBlockResponse>) -> Self {
        Self {
            reject_reason: item.as_ref().map(|x| RejectReason::from(&x.report)).unwrap_or(RejectReason::None) as i32,
            error: item.map_err(protowire::RpcError::from).err(),
        }
    }
}

impl From<&rpc_core::GetBlockTemplateRequest> for protowire::GetBlockTemplateRequestMessage {
    fn from(item: &rpc_core::GetBlockTemplateRequest) -> Self {
        Self {
            pay_address: (&item.pay_address).into(),
            extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
        }
    }
}

impl From<RpcResult<&rpc_core::GetBlockTemplateResponse>> for protowire::GetBlockTemplateResponseMessage {
    fn from(item: RpcResult<&rpc_core::GetBlockTemplateResponse>) -> Self {
        match item {
            Ok(response) => Self { block: Some((&response.block).into()), is_synced: response.is_synced, error: None },
            Err(err) => Self { block: None, is_synced: false, error: Some(err.into()) },
        }
    }
}

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

impl From<&rpc_core::NotifyNewBlockTemplateRequest> for protowire::NotifyNewBlockTemplateRequestMessage {
    fn from(item: &rpc_core::NotifyNewBlockTemplateRequest) -> Self {
        Self { command: item.command.into() }
    }
}

impl From<RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>> for protowire::NotifyNewBlockTemplateResponseMessage {
    fn from(item: RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>) -> Self {
        Self { error: item.map_err(protowire::RpcError::from).err() }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::SubmitBlockRequestMessage> for rpc_core::SubmitBlockRequest {
    type Error = RpcError;
    fn try_from(item: &protowire::SubmitBlockRequestMessage) -> RpcResult<Self> {
        if item.block.is_none() {
            return Err(RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()));
        }
        Ok(Self { block: item.block.as_ref().unwrap().try_into()?, allow_non_daa_blocks: item.allow_non_daa_blocks })
    }
}

impl From<RejectReason> for rpc_core::SubmitBlockReport {
    fn from(item: RejectReason) -> Self {
        match item {
            RejectReason::None => rpc_core::SubmitBlockReport::Success,
            RejectReason::BlockInvalid => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid),
            RejectReason::IsInIbd => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD),
        }
    }
}

impl TryFrom<&protowire::SubmitBlockResponseMessage> for rpc_core::SubmitBlockResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::SubmitBlockResponseMessage) -> RpcResult<Self> {
        Ok(Self { report: RejectReason::from_i32(item.reject_reason).ok_or(RpcError::PrimitiveToEnumConversionError)?.into() })
    }
}

impl TryFrom<&protowire::GetBlockTemplateRequestMessage> for rpc_core::GetBlockTemplateRequest {
    type Error = RpcError;
    fn try_from(item: &protowire::GetBlockTemplateRequestMessage) -> RpcResult<Self> {
        Ok(Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) })
    }
}

impl TryFrom<&protowire::GetBlockTemplateResponseMessage> for rpc_core::GetBlockTemplateResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::GetBlockTemplateResponseMessage) -> RpcResult<Self> {
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
            .map(|x| rpc_core::GetBlockTemplateResponse { block: x, is_synced: item.is_synced })
    }
}

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
    fn try_from(_: &protowire::GetInfoRequestMessage) -> RpcResult<Self> {
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

impl TryFrom<&protowire::NotifyNewBlockTemplateRequestMessage> for rpc_core::NotifyNewBlockTemplateRequest {
    type Error = RpcError;
    fn try_from(item: &protowire::NotifyNewBlockTemplateRequestMessage) -> RpcResult<Self> {
        Ok(Self { command: item.command.into() })
    }
}

impl TryFrom<&protowire::NotifyNewBlockTemplateResponseMessage> for rpc_core::NotifyNewBlockTemplateResponse {
    type Error = RpcError;
    fn try_from(item: &protowire::NotifyNewBlockTemplateResponseMessage) -> RpcResult<Self> {
        item.error.as_ref().map_or(Ok(rpc_core::NotifyNewBlockTemplateResponse {}), |x| Err(x.into()))
    }
}

// ----------------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {}
