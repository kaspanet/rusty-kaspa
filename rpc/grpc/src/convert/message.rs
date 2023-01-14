use crate::protowire::{self, submit_block_response_message::RejectReason};
use rpc_core::{RpcError, RpcExtraData, RpcHash, RpcResult};
use std::str::FromStr;

macro_rules! from {
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl From<$from_type> for $to_type {
            fn from($name: $from_type) -> Self {
                $body
            }
        }
    };
}

macro_rules! try_from {
    ($name:ident : $from_type:ty, $protowire_type:ty, $body:block) => {
        impl TryFrom<$from_type> for $protowire_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> RpcResult<Self> {
                $body
            }
        }
    };
}

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &rpc_core::SubmitBlockRequest, protowire::SubmitBlockRequestMessage, {
    Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
});

// impl From<&rpc_core::SubmitBlockRequest> for protowire::SubmitBlockRequestMessage {
//     fn from(item: &rpc_core::SubmitBlockRequest) -> Self {
//         Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
//     }
// }

from!(item: &rpc_core::SubmitBlockReport, RejectReason, {
    match item {
        rpc_core::SubmitBlockReport::Success => RejectReason::None,
        rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
        rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
    }
});

// impl From<&rpc_core::SubmitBlockReport> for RejectReason {
//     fn from(item: &rpc_core::SubmitBlockReport) -> Self {
//         match item {
//             rpc_core::SubmitBlockReport::Success => RejectReason::None,
//             rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
//             rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
//         }
//     }
// }

from!(item: RpcResult<&rpc_core::SubmitBlockResponse>, protowire::SubmitBlockResponseMessage, {
    Self {
        reject_reason: item.as_ref().map(|x| RejectReason::from(&x.report)).unwrap_or(RejectReason::None) as i32,
        error: item.map_err(protowire::RpcError::from).err(),
    }
});

// impl From<RpcResult<&rpc_core::SubmitBlockResponse>> for protowire::SubmitBlockResponseMessage {
//     fn from(item: RpcResult<&rpc_core::SubmitBlockResponse>) -> Self {
//         Self {
//             reject_reason: item.as_ref().map(|x| RejectReason::from(&x.report)).unwrap_or(RejectReason::None) as i32,
//             error: item.map_err(protowire::RpcError::from).err(),
//         }
//     }
// }

from!(item: &rpc_core::GetBlockTemplateRequest, protowire::GetBlockTemplateRequestMessage, {
    Self {
        pay_address: (&item.pay_address).into(),
        extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
    }
});

// impl From<&rpc_core::GetBlockTemplateRequest> for protowire::GetBlockTemplateRequestMessage {
//     fn from(item: &rpc_core::GetBlockTemplateRequest) -> Self {
//         Self {
//             pay_address: (&item.pay_address).into(),
//             extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
//         }
//     }
// }

from!(item: RpcResult<&rpc_core::GetBlockTemplateResponse>, protowire::GetBlockTemplateResponseMessage, {
    match item {
        Ok(response) => Self { block: Some((&response.block).into()), is_synced: response.is_synced, error: None },
        Err(err) => Self { block: None, is_synced: false, error: Some(err.into()) },
    }
});

// impl From<RpcResult<&rpc_core::GetBlockTemplateResponse>> for protowire::GetBlockTemplateResponseMessage {
//     fn from(item: RpcResult<&rpc_core::GetBlockTemplateResponse>) -> Self {
//         match item {
//             Ok(response) => Self { block: Some((&response.block).into()), is_synced: response.is_synced, error: None },
//             Err(err) => Self { block: None, is_synced: false, error: Some(err.into()) },
//         }
//     }
// }

from!(item: &rpc_core::GetBlockRequest, protowire::GetBlockRequestMessage, {
    Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
});

// impl From<&rpc_core::GetBlockRequest> for protowire::GetBlockRequestMessage {
//     fn from(item: &rpc_core::GetBlockRequest) -> Self {
//         Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
//     }
// }

from!(item: RpcResult<&rpc_core::GetBlockResponse>, protowire::GetBlockResponseMessage, {
    Self {
        block: item.as_ref().map(|x| protowire::RpcBlock::from(&x.block)).ok(),
        error: item.map_err(protowire::RpcError::from).err(),
    }
});

// impl From<RpcResult<&rpc_core::GetBlockResponse>> for protowire::GetBlockResponseMessage {
//     fn from(item: RpcResult<&rpc_core::GetBlockResponse>) -> Self {
//         Self {
//             block: item.as_ref().map(|x| protowire::RpcBlock::from(&x.block)).ok(),
//             error: item.map_err(protowire::RpcError::from).err(),
//         }
//     }
// }

from!(item: &rpc_core::NotifyBlockAddedRequest, protowire::NotifyBlockAddedRequestMessage, {
    //
    Self { command: item.command.into() }
});

// impl From<&rpc_core::NotifyBlockAddedRequest> for protowire::NotifyBlockAddedRequestMessage {
//     fn from(item: &rpc_core::NotifyBlockAddedRequest) -> Self {
//         Self { command: item.command.into() }
//     }
// }

from!(item: RpcResult<&rpc_core::NotifyBlockAddedResponse>, protowire::NotifyBlockAddedResponseMessage, {
    Self { error: item.map_err(protowire::RpcError::from).err() }
});

// impl From<RpcResult<&rpc_core::NotifyBlockAddedResponse>> for protowire::NotifyBlockAddedResponseMessage {
//     fn from(item: RpcResult<&rpc_core::NotifyBlockAddedResponse>) -> Self {
//         Self { error: item.map_err(protowire::RpcError::from).err() }
//     }
// }

from!(_item: &rpc_core::GetInfoRequest, protowire::GetInfoRequestMessage, { Self {} });

// impl From<&rpc_core::GetInfoRequest> for protowire::GetInfoRequestMessage {
//     fn from(_item: &rpc_core::GetInfoRequest) -> Self {
//         Self {}
//     }
// }

from!(item: RpcResult<&rpc_core::GetInfoResponse>, protowire::GetInfoResponseMessage, {
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
});

// impl From<RpcResult<&rpc_core::GetInfoResponse>> for protowire::GetInfoResponseMessage {
//     fn from(item: RpcResult<&rpc_core::GetInfoResponse>) -> Self {
//         match item {
//             Ok(response) => Self {
//                 p2p_id: response.p2p_id.clone(),
//                 mempool_size: response.mempool_size,
//                 server_version: response.server_version.clone(),
//                 is_utxo_indexed: response.is_utxo_indexed,
//                 is_synced: response.is_synced,
//                 has_notify_command: response.has_notify_command,
//                 error: None,
//             },
//             Err(err) => Self {
//                 p2p_id: String::default(),
//                 mempool_size: 0,
//                 server_version: String::default(),
//                 is_utxo_indexed: false,
//                 is_synced: false,
//                 has_notify_command: false,
//                 error: Some(err.into()),
//             },
//         }
//     }
// }

from!(item: &rpc_core::NotifyNewBlockTemplateRequest, protowire::NotifyNewBlockTemplateRequestMessage, {
    Self { command: item.command.into() }
});

// impl From<&rpc_core::NotifyNewBlockTemplateRequest> for protowire::NotifyNewBlockTemplateRequestMessage {
//     fn from(item: &rpc_core::NotifyNewBlockTemplateRequest) -> Self {
//         Self { command: item.command.into() }
//     }
// }

from!(item: RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>, protowire::NotifyNewBlockTemplateResponseMessage, {
    Self { error: item.map_err(protowire::RpcError::from).err() }
});

// impl From<RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>> for protowire::NotifyNewBlockTemplateResponseMessage {
//     fn from(item: RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>) -> Self {
//         Self { error: item.map_err(protowire::RpcError::from).err() }
//     }
// }

// ~~~

from!(_item: &rpc_core::GetCurrentNetworkRequest, protowire::GetCurrentNetworkRequestMessage, { Self {} });

from!(_item: RpcResult<&rpc_core::GetCurrentNetworkResponse>, protowire::GetCurrentNetworkResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetPeerAddressesRequest, protowire::GetPeerAddressesRequestMessage, { Self {} });

from!(_item: RpcResult<&rpc_core::GetPeerAddressesResponse>, protowire::GetPeerAddressesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetSelectedTipHashRequest, protowire::GetSelectedTipHashRequestMessage, { Self {} });

from!(_item: RpcResult<&rpc_core::GetSelectedTipHashResponse>, protowire::GetSelectedTipHashResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetMempoolEntryRequest, protowire::GetMempoolEntryRequestMessage, {
    //
    unimplemented!()
});

from!(_item: RpcResult<&rpc_core::GetMempoolEntryResponse>, protowire::GetMempoolEntryResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetMempoolEntriesRequest, protowire::GetMempoolEntriesRequestMessage, {
    //
    unimplemented!()
});

from!(_item: RpcResult<&rpc_core::GetMempoolEntriesResponse>, protowire::GetMempoolEntriesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetConnectedPeerInfoRequest, protowire::GetConnectedPeerInfoRequestMessage, {
    //
    unimplemented!()
});

from!(_item: RpcResult<&rpc_core::GetConnectedPeerInfoResponse>, protowire::GetConnectedPeerInfoResponseMessage, {
    //
    unimplemented!()
});

from!(_item: &rpc_core::AddPeerRequest, protowire::AddPeerRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::AddPeerResponse>, protowire::AddPeerResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::SubmitTransactionRequest, protowire::SubmitTransactionRequestMessage, {
    //
    unimplemented!()
});

from!(_item: RpcResult<&rpc_core::SubmitTransactionResponse>, protowire::SubmitTransactionResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetSubnetworkRequest, protowire::GetSubnetworkRequestMessage, {
    //
    unimplemented!()
});

from!(_item: RpcResult<&rpc_core::GetSubnetworkResponse>, protowire::GetSubnetworkResponseMessage, {
    unimplemented!();
});

// ~~~

from!(
    _item: &rpc_core::GetVirtualSelectedParentChainFromBlockRequest,
    protowire::GetVirtualSelectedParentChainFromBlockRequestMessage,
    {
        unimplemented!();
    }
);

from!(
    _item: RpcResult<&rpc_core::GetVirtualSelectedParentChainFromBlockResponse>,
    protowire::GetVirtualSelectedParentChainFromBlockResponseMessage,
    {
        unimplemented!();
    }
);

from!(_item: &rpc_core::GetBlocksRequest, protowire::GetBlocksRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetBlocksResponse>, protowire::GetBlocksResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetBlockCountRequest, protowire::GetBlockCountRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetBlockCountResponse>, protowire::GetBlockCountResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetBlockDagInfoRequest, protowire::GetBlockDagInfoRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetBlockDagInfoResponse>, protowire::GetBlockDagInfoResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::ResolveFinalityConflictRequest, protowire::ResolveFinalityConflictRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::ResolveFinalityConflictResponse>, protowire::ResolveFinalityConflictResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::ShutdownRequest, protowire::ShutdownRequestMessage, {
    // unimplemented!();
    Self {}
});

from!(item: RpcResult<&rpc_core::ShutdownResponse>, protowire::ShutdownResponseMessage, {
    // unimplemented!();
    Self { error: item.map_err(protowire::RpcError::from).err() }
});

from!(_item: &rpc_core::GetHeadersRequest, protowire::GetHeadersRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetHeadersResponse>, protowire::GetHeadersResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetUtxosByAddressesRequest, protowire::GetUtxosByAddressesRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetUtxosByAddressesResponse>, protowire::GetUtxosByAddressesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetBalanceByAddressRequest, protowire::GetBalanceByAddressRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetBalanceByAddressResponse>, protowire::GetBalanceByAddressResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetBalancesByAddressesRequest, protowire::GetBalancesByAddressesRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetBalancesByAddressesResponse>, protowire::GetBalancesByAddressesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetVirtualSelectedParentBlueScoreRequest, protowire::GetVirtualSelectedParentBlueScoreRequestMessage, {
    unimplemented!();
});

from!(
    _item: RpcResult<&rpc_core::GetVirtualSelectedParentBlueScoreResponse>,
    protowire::GetVirtualSelectedParentBlueScoreResponseMessage,
    {
        unimplemented!();
    }
);

from!(_item: &rpc_core::BanRequest, protowire::BanRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::BanResponse>, protowire::BanResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::UnbanRequest, protowire::UnbanRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::UnbanResponse>, protowire::UnbanResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::EstimateNetworkHashesPerSecondRequest, protowire::EstimateNetworkHashesPerSecondRequestMessage, {
    unimplemented!();
});

from!(
    _item: RpcResult<&rpc_core::EstimateNetworkHashesPerSecondResponse>,
    protowire::EstimateNetworkHashesPerSecondResponseMessage,
    {
        unimplemented!();
    }
);

from!(_item: &rpc_core::GetMempoolEntriesByAddressesRequest, protowire::GetMempoolEntriesByAddressesRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetMempoolEntriesByAddressesResponse>, protowire::GetMempoolEntriesByAddressesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetCoinSupplyRequest, protowire::GetCoinSupplyRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetCoinSupplyResponse>, protowire::GetCoinSupplyResponseMessage, {
    unimplemented!();
});


from!(_item: &rpc_core::PingRequest, protowire::PingRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::PingResponse>, protowire::PingResponseMessage, {
    unimplemented!();
});


from!(_item: &rpc_core::GetProcessMetricsRequest, protowire::GetProcessMetricsRequestMessage, {
    unimplemented!();
});

from!(_item: RpcResult<&rpc_core::GetProcessMetricsResponse>, protowire::GetProcessMetricsResponseMessage, {
    unimplemented!();
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::SubmitBlockRequestMessage, rpc_core::SubmitBlockRequest, {
    if item.block.is_none() {
        return Err(RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()));
    }
    Ok(Self { block: item.block.as_ref().unwrap().try_into()?, allow_non_daa_blocks: item.allow_non_daa_blocks })
});

// impl TryFrom<&protowire::SubmitBlockRequestMessage> for rpc_core::SubmitBlockRequest {
//     type Error = RpcError;
//     fn try_from(item: &protowire::SubmitBlockRequestMessage) -> RpcResult<Self> {
//         if item.block.is_none() {
//             return Err(RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()));
//         }
//         Ok(Self { block: item.block.as_ref().unwrap().try_into()?, allow_non_daa_blocks: item.allow_non_daa_blocks })
//     }
// }

from!(item: RejectReason, rpc_core::SubmitBlockReport, {
    match item {
        RejectReason::None => rpc_core::SubmitBlockReport::Success,
        RejectReason::BlockInvalid => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid),
        RejectReason::IsInIbd => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD),
    }
});

// impl From<RejectReason> for rpc_core::SubmitBlockReport {
//     fn from(item: RejectReason) -> Self {
//         match item {
//             RejectReason::None => rpc_core::SubmitBlockReport::Success,
//             RejectReason::BlockInvalid => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid),
//             RejectReason::IsInIbd => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD),
//         }
//     }
// }

try_from!(item: &protowire::SubmitBlockResponseMessage, rpc_core::SubmitBlockResponse, {
    Ok(Self { report: RejectReason::from_i32(item.reject_reason).ok_or(RpcError::PrimitiveToEnumConversionError)?.into() })
});

// impl TryFrom<&protowire::SubmitBlockResponseMessage> for rpc_core::SubmitBlockResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::SubmitBlockResponseMessage) -> RpcResult<Self> {
//         Ok(Self { report: RejectReason::from_i32(item.reject_reason).ok_or(RpcError::PrimitiveToEnumConversionError)?.into() })
//     }
// }

try_from!(item: &protowire::GetBlockTemplateRequestMessage, rpc_core::GetBlockTemplateRequest, {
    Ok(Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) })
});

// impl TryFrom<&protowire::GetBlockTemplateRequestMessage> for rpc_core::GetBlockTemplateRequest {
//     type Error = RpcError;
//     fn try_from(item: &protowire::GetBlockTemplateRequestMessage) -> RpcResult<Self> {
//         Ok(Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) })
//     }
// }

try_from!(item: &protowire::GetBlockTemplateResponseMessage, rpc_core::GetBlockTemplateResponse, {
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
});

// impl TryFrom<&protowire::GetBlockTemplateResponseMessage> for rpc_core::GetBlockTemplateResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::GetBlockTemplateResponseMessage) -> RpcResult<Self> {
//         item.block
//             .as_ref()
//             .map_or_else(
//                 || {
//                     item.error
//                         .as_ref()
//                         .map_or(Err(RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "error".to_string())), |x| {
//                             Err(x.into())
//                         })
//                 },
//                 rpc_core::RpcBlock::try_from,
//             )
//             .map(|x| rpc_core::GetBlockTemplateResponse { block: x, is_synced: item.is_synced })
//     }
// }

try_from!(item: &protowire::GetBlockRequestMessage, rpc_core::GetBlockRequest, {
    Ok(Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions })
});

// impl TryFrom<&protowire::GetBlockRequestMessage> for rpc_core::GetBlockRequest {
//     type Error = RpcError;
//     fn try_from(item: &protowire::GetBlockRequestMessage) -> RpcResult<Self> {
//         Ok(Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions })
//     }
// }

try_from!(item: &protowire::GetBlockResponseMessage, rpc_core::GetBlockResponse, {
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
});

// impl TryFrom<&protowire::GetBlockResponseMessage> for rpc_core::GetBlockResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::GetBlockResponseMessage) -> RpcResult<Self> {
//         item.block
//             .as_ref()
//             .map_or_else(
//                 || {
//                     item.error
//                         .as_ref()
//                         .map_or(Err(RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "error".to_string())), |x| {
//                             Err(x.into())
//                         })
//                 },
//                 rpc_core::RpcBlock::try_from,
//             )
//             .map(|x| rpc_core::GetBlockResponse { block: x })
//     }
// }

try_from!(item: &protowire::NotifyBlockAddedRequestMessage, rpc_core::NotifyBlockAddedRequest, {
    Ok(Self { command: item.command.into() })
});

// impl TryFrom<&protowire::NotifyBlockAddedRequestMessage> for rpc_core::NotifyBlockAddedRequest {
//     type Error = RpcError;
//     fn try_from(item: &protowire::NotifyBlockAddedRequestMessage) -> RpcResult<Self> {
//         Ok(Self { command: item.command.into() })
//     }
// }

try_from!(item: &protowire::NotifyBlockAddedResponseMessage, rpc_core::NotifyBlockAddedResponse, {
    item.error.as_ref().map_or(Ok(rpc_core::NotifyBlockAddedResponse {}), |x| Err(x.into()))
});

// impl TryFrom<&protowire::NotifyBlockAddedResponseMessage> for rpc_core::NotifyBlockAddedResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::NotifyBlockAddedResponseMessage) -> RpcResult<Self> {
//         item.error.as_ref().map_or(Ok(rpc_core::NotifyBlockAddedResponse {}), |x| Err(x.into()))
//     }
// }

try_from!(_item: &protowire::GetInfoRequestMessage, rpc_core::GetInfoRequest, { Ok(Self {}) });

// impl TryFrom<&protowire::GetInfoRequestMessage> for rpc_core::GetInfoRequest {
//     type Error = RpcError;
//     fn try_from(_: &protowire::GetInfoRequestMessage) -> RpcResult<Self> {
//         Ok(Self {})
//     }
// }

try_from!(item: &protowire::GetInfoResponseMessage, rpc_core::GetInfoResponse, {
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
});

// impl TryFrom<&protowire::GetInfoResponseMessage> for rpc_core::GetInfoResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::GetInfoResponseMessage) -> RpcResult<Self> {
//         if let Some(err) = item.error.as_ref() {
//             Err(err.into())
//         } else {
//             Ok(Self {
//                 p2p_id: item.p2p_id.clone(),
//                 mempool_size: item.mempool_size,
//                 server_version: item.server_version.clone(),
//                 is_utxo_indexed: item.is_utxo_indexed,
//                 is_synced: item.is_synced,
//                 has_notify_command: item.has_notify_command,
//             })
//         }
//     }
// }

try_from!(item: &protowire::NotifyNewBlockTemplateRequestMessage, rpc_core::NotifyNewBlockTemplateRequest, {
    Ok(Self { command: item.command.into() })
});

// impl TryFrom<&protowire::NotifyNewBlockTemplateRequestMessage> for rpc_core::NotifyNewBlockTemplateRequest {
//     type Error = RpcError;
//     fn try_from(item: &protowire::NotifyNewBlockTemplateRequestMessage) -> RpcResult<Self> {
//         Ok(Self { command: item.command.into() })
//     }
// }

try_from!(item: &protowire::NotifyNewBlockTemplateResponseMessage, rpc_core::NotifyNewBlockTemplateResponse, {
    item.error.as_ref().map_or(Ok(rpc_core::NotifyNewBlockTemplateResponse {}), |x| Err(x.into()))
});

// impl TryFrom<&protowire::NotifyNewBlockTemplateResponseMessage> for rpc_core::NotifyNewBlockTemplateResponse {
//     type Error = RpcError;
//     fn try_from(item: &protowire::NotifyNewBlockTemplateResponseMessage) -> RpcResult<Self> {
//         item.error.as_ref().map_or(Ok(rpc_core::NotifyNewBlockTemplateResponse {}), |x| Err(x.into()))
//     }
// }

// ~~~

try_from!(_item: &protowire::GetCurrentNetworkRequestMessage, rpc_core::GetCurrentNetworkRequest, { unimplemented!() });

try_from!(_item: &protowire::GetCurrentNetworkResponseMessage, rpc_core::GetCurrentNetworkResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetPeerAddressesRequestMessage, rpc_core::GetPeerAddressesRequest, { unimplemented!() });

try_from!(_item: &protowire::GetPeerAddressesResponseMessage, rpc_core::GetPeerAddressesResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSelectedTipHashRequestMessage, rpc_core::GetSelectedTipHashRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSelectedTipHashResponseMessage, rpc_core::GetSelectedTipHashResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntryRequestMessage, rpc_core::GetMempoolEntryRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntryResponseMessage, rpc_core::GetMempoolEntryResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesRequestMessage, rpc_core::GetMempoolEntriesRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesResponseMessage, rpc_core::GetMempoolEntriesResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetConnectedPeerInfoRequestMessage, rpc_core::GetConnectedPeerInfoRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetConnectedPeerInfoResponseMessage, rpc_core::GetConnectedPeerInfoResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::AddPeerRequestMessage, rpc_core::AddPeerRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::AddPeerResponseMessage, rpc_core::AddPeerResponse, { unimplemented!() });

try_from!(_item: &protowire::SubmitTransactionRequestMessage, rpc_core::SubmitTransactionRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::SubmitTransactionResponseMessage, rpc_core::SubmitTransactionResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSubnetworkRequestMessage, rpc_core::GetSubnetworkRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSubnetworkResponseMessage, rpc_core::GetSubnetworkResponse, {
    //
    unimplemented!()
});

try_from!(
    _item: &protowire::GetVirtualSelectedParentChainFromBlockRequestMessage,
    rpc_core::GetVirtualSelectedParentChainFromBlockRequest,
    { unimplemented!() }
);

try_from!(
    _item: &protowire::GetVirtualSelectedParentChainFromBlockResponseMessage,
    rpc_core::GetVirtualSelectedParentChainFromBlockResponse,
    { unimplemented!() }
);

try_from!(_item: &protowire::GetBlocksRequestMessage, rpc_core::GetBlocksRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlocksResponseMessage, rpc_core::GetBlocksResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockCountRequestMessage, rpc_core::GetBlockCountRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockCountResponseMessage, rpc_core::GetBlockCountResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockDagInfoRequestMessage, rpc_core::GetBlockDagInfoRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockDagInfoResponseMessage, rpc_core::GetBlockDagInfoResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::ResolveFinalityConflictRequestMessage, rpc_core::ResolveFinalityConflictRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::ResolveFinalityConflictResponseMessage, rpc_core::ResolveFinalityConflictResponse, { unimplemented!() });

try_from!(_item: &protowire::ShutdownRequestMessage, rpc_core::ShutdownRequest, {
    // unimplemented!()
    Ok(Self {})
});

try_from!(_item: &protowire::ShutdownResponseMessage, rpc_core::ShutdownResponse, {
    // unimplemented!()
    Ok(Self {})
});

try_from!(_item: &protowire::GetHeadersRequestMessage, rpc_core::GetHeadersRequest, { unimplemented!() });

try_from!(_item: &protowire::GetHeadersResponseMessage, rpc_core::GetHeadersResponse, { unimplemented!() });

try_from!(_item: &protowire::GetUtxosByAddressesRequestMessage, rpc_core::GetUtxosByAddressesRequest, { unimplemented!() });

try_from!(_item: &protowire::GetUtxosByAddressesResponseMessage, rpc_core::GetUtxosByAddressesResponse, { unimplemented!() });

try_from!(_item: &protowire::GetBalanceByAddressRequestMessage, rpc_core::GetBalanceByAddressRequest, { unimplemented!() });

try_from!(_item: &protowire::GetBalanceByAddressResponseMessage, rpc_core::GetBalanceByAddressResponse, { unimplemented!() });

try_from!(_item: &protowire::GetBalancesByAddressesRequestMessage, rpc_core::GetBalancesByAddressesRequest, { unimplemented!() });

try_from!(_item: &protowire::GetBalancesByAddressesResponseMessage, rpc_core::GetBalancesByAddressesResponse, { unimplemented!() });

try_from!(_item: &protowire::GetVirtualSelectedParentBlueScoreRequestMessage, rpc_core::GetVirtualSelectedParentBlueScoreRequest, {
    unimplemented!()
});

try_from!(_item: &protowire::GetVirtualSelectedParentBlueScoreResponseMessage, rpc_core::GetVirtualSelectedParentBlueScoreResponse, {
    unimplemented!()
});

try_from!(_item: &protowire::BanRequestMessage, rpc_core::BanRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::BanResponseMessage, rpc_core::BanResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::UnbanRequestMessage, rpc_core::UnbanRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::UnbanResponseMessage, rpc_core::UnbanResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::EstimateNetworkHashesPerSecondRequestMessage, rpc_core::EstimateNetworkHashesPerSecondRequest, {
    unimplemented!()
});

try_from!(_item: &protowire::EstimateNetworkHashesPerSecondResponseMessage, rpc_core::EstimateNetworkHashesPerSecondResponse, {
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesByAddressesRequestMessage, rpc_core::GetMempoolEntriesByAddressesRequest, {
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesByAddressesResponseMessage, rpc_core::GetMempoolEntriesByAddressesResponse, {
    unimplemented!()
});

try_from!(_item: &protowire::GetCoinSupplyRequestMessage, rpc_core::GetCoinSupplyRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetCoinSupplyResponseMessage, rpc_core::GetCoinSupplyResponse, {
    //
    unimplemented!()
});


try_from!(_item: &protowire::PingRequestMessage, rpc_core::PingRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::PingResponseMessage, rpc_core::PingResponse, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetProcessMetricsRequestMessage, rpc_core::GetProcessMetricsRequest, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetProcessMetricsResponseMessage, rpc_core::GetProcessMetricsResponse, {
    //
    unimplemented!()
});

// ----------------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------------

// TODO: tests
