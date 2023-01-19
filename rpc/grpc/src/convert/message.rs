use crate::protowire::{self, submit_block_response_message::RejectReason};
use rpc_core::{RpcError, RpcExtraData, RpcHash, RpcResult};
use std::str::FromStr;

macro_rules! from {
    // Response capture
    ($name:ident : RpcResult<&$from_type:ty>, $to_type:ty, $ctor:block) => {
        impl From<RpcResult<&$from_type>> for $to_type {
            fn from(item: RpcResult<&$from_type>) -> Self {
                match item {
                    Ok($name) => $ctor,
                    Err(err) => {
                        let mut message = Self::default();
                        message.error = Some(err.into());
                        message
                    }
                }
            }
        }
    };

    // Response without parameter capture
    (RpcResult<&$from_type:ty>, $to_type:ty) => {
        impl From<RpcResult<&$from_type>> for $to_type {
            fn from(item: RpcResult<&$from_type>) -> Self {
                Self { error: item.map_err(protowire::RpcError::from).err() }
            }
        }
    };

    // Request and other capture
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl From<$from_type> for $to_type {
            fn from($name: $from_type) -> Self {
                $body
            }
        }
    };

    // Request and other without parameter capture
    ($from_type:ty, $to_type:ty) => {
        impl From<$from_type> for $to_type {
            fn from(_: $from_type) -> Self {
                Self {}
            }
        }
    };
}

macro_rules! try_from {
    // Response capture
    ($name:ident : $from_type:ty, RpcResult<$to_type:ty>, $ctor:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> RpcResult<Self> {
                if let Some(ref err) = $name.error {
                    Err(err.into())
                } else {
                    #[allow(unreachable_code)] // TODO: remove attribute when all converters are implemented
                    Ok($ctor)
                }
            }
        }
    };

    // Response without parameter capture
    ($from_type:ty, RpcResult<$to_type:ty>) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from(item: $from_type) -> RpcResult<Self> {
                item.error.as_ref().map_or(Ok(Self {}), |x| Err(x.into()))
            }
        }
    };

    // Request and other capture
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> RpcResult<Self> {
                #[allow(unreachable_code)] // TODO: remove attribute when all converters are implemented
                Ok($body)
            }
        }
    };

    // Request and other without parameter capture
    ($from_type:ty, $to_type:ty) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from(_: $from_type) -> RpcResult<Self> {
                Ok(Self {})
            }
        }
    };
}

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &rpc_core::SubmitBlockReport, RejectReason, {
    match item {
        rpc_core::SubmitBlockReport::Success => RejectReason::None,
        rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
        rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
    }
});

from!(item: &rpc_core::SubmitBlockRequest, protowire::SubmitBlockRequestMessage, {
    Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
});
from!(item: RpcResult<&rpc_core::SubmitBlockResponse>, protowire::SubmitBlockResponseMessage, {
    Self { reject_reason: RejectReason::from(&item.report) as i32, error: None }
});

from!(item: &rpc_core::GetBlockTemplateRequest, protowire::GetBlockTemplateRequestMessage, {
    Self {
        pay_address: (&item.pay_address).into(),
        extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
    }
});
from!(item: RpcResult<&rpc_core::GetBlockTemplateResponse>, protowire::GetBlockTemplateResponseMessage, {
    Self { block: Some((&item.block).into()), is_synced: item.is_synced, error: None }
});

from!(item: &rpc_core::GetBlockRequest, protowire::GetBlockRequestMessage, {
    Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
});
from!(item: RpcResult<&rpc_core::GetBlockResponse>, protowire::GetBlockResponseMessage, {
    Self { block: Some((&item.block).into()), error: None }
});

from!(item: &rpc_core::NotifyBlockAddedRequest, protowire::NotifyBlockAddedRequestMessage, { Self { command: item.command.into() } });
from!(RpcResult<&rpc_core::NotifyBlockAddedResponse>, protowire::NotifyBlockAddedResponseMessage);

from!(&rpc_core::GetInfoRequest, protowire::GetInfoRequestMessage);
from!(item: RpcResult<&rpc_core::GetInfoResponse>, protowire::GetInfoResponseMessage, {
    Self {
        p2p_id: item.p2p_id.clone(),
        mempool_size: item.mempool_size,
        server_version: item.server_version.clone(),
        is_utxo_indexed: item.is_utxo_indexed,
        is_synced: item.is_synced,
        has_notify_command: item.has_notify_command,
        has_message_id: item.has_message_id,
        error: None,
    }
});

from!(item: &rpc_core::NotifyNewBlockTemplateRequest, protowire::NotifyNewBlockTemplateRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&rpc_core::NotifyNewBlockTemplateResponse>, protowire::NotifyNewBlockTemplateResponseMessage);

// ~~~

from!(&rpc_core::GetCurrentNetworkRequest, protowire::GetCurrentNetworkRequestMessage);
from!(_item: RpcResult<&rpc_core::GetCurrentNetworkResponse>, protowire::GetCurrentNetworkResponseMessage, {
    unimplemented!();
});

from!(&rpc_core::GetPeerAddressesRequest, protowire::GetPeerAddressesRequestMessage);
from!(_item: RpcResult<&rpc_core::GetPeerAddressesResponse>, protowire::GetPeerAddressesResponseMessage, {
    unimplemented!();
});

from!(&rpc_core::GetSelectedTipHashRequest, protowire::GetSelectedTipHashRequestMessage);
from!(_item: RpcResult<&rpc_core::GetSelectedTipHashResponse>, protowire::GetSelectedTipHashResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetMempoolEntryRequest, protowire::GetMempoolEntryRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&rpc_core::GetMempoolEntryResponse>, protowire::GetMempoolEntryResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetMempoolEntriesRequest, protowire::GetMempoolEntriesRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&rpc_core::GetMempoolEntriesResponse>, protowire::GetMempoolEntriesResponseMessage, {
    unimplemented!();
});

from!(_item: &rpc_core::GetConnectedPeerInfoRequest, protowire::GetConnectedPeerInfoRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&rpc_core::GetConnectedPeerInfoResponse>, protowire::GetConnectedPeerInfoResponseMessage, { unimplemented!() });

from!(_item: &rpc_core::AddPeerRequest, protowire::AddPeerRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&rpc_core::AddPeerResponse>, protowire::AddPeerResponseMessage, {
    unimplemented!();
});

from!(item: &rpc_core::SubmitTransactionRequest, protowire::SubmitTransactionRequestMessage, {
    Self { transaction: Some((&item.transaction).into()), allow_orphan: item.allow_orphan }
});
from!(item: RpcResult<&rpc_core::SubmitTransactionResponse>, protowire::SubmitTransactionResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), error: None }
});

from!(_item: &rpc_core::GetSubnetworkRequest, protowire::GetSubnetworkRequestMessage, { unimplemented!() });
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

from!(item: &rpc_core::GetBlocksRequest, protowire::GetBlocksRequestMessage, {
    Self { low_hash: item.low_hash.to_string(), include_blocks: item.include_blocks, include_transactions: item.include_transactions }
});
from!(item: RpcResult<&rpc_core::GetBlocksResponse>, protowire::GetBlocksResponseMessage, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        blocks: item.blocks.iter().map(|x| x.into()).collect::<Vec<_>>(),
        error: None,
    }
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

from!(&rpc_core::ShutdownRequest, protowire::ShutdownRequestMessage);
from!(RpcResult<&rpc_core::ShutdownResponse>, protowire::ShutdownResponseMessage);

from!(_item: &rpc_core::GetHeadersRequest, protowire::GetHeadersRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&rpc_core::GetHeadersResponse>, protowire::GetHeadersResponseMessage, {
    unimplemented!();
});

from!(item: &rpc_core::GetUtxosByAddressesRequest, protowire::GetUtxosByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&rpc_core::GetUtxosByAddressesResponse>, protowire::GetUtxosByAddressesResponseMessage, {
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
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

from!(&rpc_core::GetVirtualSelectedParentBlueScoreRequest, protowire::GetVirtualSelectedParentBlueScoreRequestMessage);
from!(
    item: RpcResult<&rpc_core::GetVirtualSelectedParentBlueScoreResponse>,
    protowire::GetVirtualSelectedParentBlueScoreResponseMessage,
    { Self { blue_score: item.blue_score, error: None } }
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

from!(item: &rpc_core::NotifyUtxosChangedRequest, protowire::NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect(), command: item.command.into() }
});
from!(RpcResult<&rpc_core::NotifyUtxosChangedResponse>, protowire::NotifyUtxosChangedResponseMessage);

from!(item: &rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&rpc_core::NotifyPruningPointUtxoSetOverrideResponse>, protowire::NotifyPruningPointUtxoSetOverrideResponseMessage);

from!(item: &rpc_core::NotifyFinalityConflictRequest, protowire::NotifyFinalityConflictRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&rpc_core::NotifyFinalityConflictResponse>, protowire::NotifyFinalityConflictResponseMessage);

from!(item: &rpc_core::NotifyVirtualDaaScoreChangedRequest, protowire::NotifyVirtualDaaScoreChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&rpc_core::NotifyVirtualDaaScoreChangedResponse>, protowire::NotifyVirtualDaaScoreChangedResponseMessage);

from!(
    item: &rpc_core::NotifyVirtualSelectedParentChainChangedRequest,
    protowire::NotifyVirtualSelectedParentChainChangedRequestMessage,
    { Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() } }
);
from!(
    RpcResult<&rpc_core::NotifyVirtualSelectedParentChainChangedResponse>,
    protowire::NotifyVirtualSelectedParentChainChangedResponseMessage
);

from!(
    item: &rpc_core::NotifyVirtualSelectedParentBlueScoreChangedRequest,
    protowire::NotifyVirtualSelectedParentBlueScoreChangedRequestMessage,
    { Self { command: item.command.into() } }
);
from!(
    RpcResult<&rpc_core::NotifyVirtualSelectedParentBlueScoreChangedResponse>,
    protowire::NotifyVirtualSelectedParentBlueScoreChangedResponseMessage
);

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

from!(item: RejectReason, rpc_core::SubmitBlockReport, {
    match item {
        RejectReason::None => rpc_core::SubmitBlockReport::Success,
        RejectReason::BlockInvalid => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::BlockInvalid),
        RejectReason::IsInIbd => rpc_core::SubmitBlockReport::Reject(rpc_core::SubmitBlockRejectReason::IsInIBD),
    }
});

try_from!(item: &protowire::SubmitBlockRequestMessage, rpc_core::SubmitBlockRequest, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()))?
            .try_into()?,
        allow_non_daa_blocks: item.allow_non_daa_blocks,
    }
});
try_from!(item: &protowire::SubmitBlockResponseMessage, RpcResult<rpc_core::SubmitBlockResponse>, {
    Self { report: RejectReason::from_i32(item.reject_reason).ok_or(RpcError::PrimitiveToEnumConversionError)?.into() }
});

try_from!(item: &protowire::GetBlockTemplateRequestMessage, rpc_core::GetBlockTemplateRequest, {
    Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) }
});
try_from!(item: &protowire::GetBlockTemplateResponseMessage, RpcResult<rpc_core::GetBlockTemplateResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockTemplateResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
        is_synced: item.is_synced,
    }
});

try_from!(item: &protowire::GetBlockRequestMessage, rpc_core::GetBlockRequest, {
    Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions }
});
try_from!(item: &protowire::GetBlockResponseMessage, RpcResult<rpc_core::GetBlockResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::NotifyBlockAddedRequestMessage, rpc_core::NotifyBlockAddedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyBlockAddedResponseMessage, RpcResult<rpc_core::NotifyBlockAddedResponse>);

try_from!(&protowire::GetInfoRequestMessage, rpc_core::GetInfoRequest);
try_from!(item: &protowire::GetInfoResponseMessage, RpcResult<rpc_core::GetInfoResponse>, {
    Self {
        p2p_id: item.p2p_id.clone(),
        mempool_size: item.mempool_size,
        server_version: item.server_version.clone(),
        is_utxo_indexed: item.is_utxo_indexed,
        is_synced: item.is_synced,
        has_notify_command: item.has_notify_command,
        has_message_id: item.has_message_id,
    }
});

try_from!(item: &protowire::NotifyNewBlockTemplateRequestMessage, rpc_core::NotifyNewBlockTemplateRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyNewBlockTemplateResponseMessage, RpcResult<rpc_core::NotifyNewBlockTemplateResponse>);

// ~~~

try_from!(_item: &protowire::GetCurrentNetworkRequestMessage, rpc_core::GetCurrentNetworkRequest, { unimplemented!() });
try_from!(_item: &protowire::GetCurrentNetworkResponseMessage, RpcResult<rpc_core::GetCurrentNetworkResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetPeerAddressesRequestMessage, rpc_core::GetPeerAddressesRequest, { unimplemented!() });
try_from!(_item: &protowire::GetPeerAddressesResponseMessage, RpcResult<rpc_core::GetPeerAddressesResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSelectedTipHashRequestMessage, rpc_core::GetSelectedTipHashRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetSelectedTipHashResponseMessage, RpcResult<rpc_core::GetSelectedTipHashResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntryRequestMessage, rpc_core::GetMempoolEntryRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetMempoolEntryResponseMessage, RpcResult<rpc_core::GetMempoolEntryResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesRequestMessage, rpc_core::GetMempoolEntriesRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetMempoolEntriesResponseMessage, RpcResult<rpc_core::GetMempoolEntriesResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetConnectedPeerInfoRequestMessage, rpc_core::GetConnectedPeerInfoRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetConnectedPeerInfoResponseMessage, RpcResult<rpc_core::GetConnectedPeerInfoResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::AddPeerRequestMessage, rpc_core::AddPeerRequest, {
    //
    unimplemented!()
});
try_from!(item: &protowire::AddPeerResponseMessage, RpcResult<rpc_core::AddPeerResponse>, { unimplemented!() });

try_from!(item: &protowire::SubmitTransactionRequestMessage, rpc_core::SubmitTransactionRequest, {
    Self {
        transaction: item
            .transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionRequestMessage".to_string(), "transaction".to_string()))?
            .try_into()?,
        allow_orphan: item.allow_orphan,
    }
});
try_from!(item: &protowire::SubmitTransactionResponseMessage, RpcResult<rpc_core::SubmitTransactionResponse>, {
    Self { transaction_id: RpcHash::from_str(&item.transaction_id)? }
});

try_from!(_item: &protowire::GetSubnetworkRequestMessage, rpc_core::GetSubnetworkRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetSubnetworkResponseMessage, RpcResult<rpc_core::GetSubnetworkResponse>, {
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
    RpcResult<rpc_core::GetVirtualSelectedParentChainFromBlockResponse>,
    { unimplemented!() }
);

try_from!(item: &protowire::GetBlocksRequestMessage, rpc_core::GetBlocksRequest, {
    Self {
        low_hash: RpcHash::from_str(&item.low_hash)?,
        include_blocks: item.include_blocks,
        include_transactions: item.include_transactions,
    }
});
try_from!(item: &protowire::GetBlocksResponseMessage, RpcResult<rpc_core::GetBlocksResponse>, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        blocks: item.blocks.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(_item: &protowire::GetBlockCountRequestMessage, rpc_core::GetBlockCountRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetBlockCountResponseMessage, RpcResult<rpc_core::GetBlockCountResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockDagInfoRequestMessage, rpc_core::GetBlockDagInfoRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetBlockDagInfoResponseMessage, RpcResult<rpc_core::GetBlockDagInfoResponse>, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::ResolveFinalityConflictRequestMessage, rpc_core::ResolveFinalityConflictRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::ResolveFinalityConflictResponseMessage, RpcResult<rpc_core::ResolveFinalityConflictResponse>, {
    unimplemented!()
});

try_from!(&protowire::ShutdownRequestMessage, rpc_core::ShutdownRequest);
try_from!(&protowire::ShutdownResponseMessage, RpcResult<rpc_core::ShutdownResponse>);

try_from!(_item: &protowire::GetHeadersRequestMessage, rpc_core::GetHeadersRequest, { unimplemented!() });
try_from!(_item: &protowire::GetHeadersResponseMessage, RpcResult<rpc_core::GetHeadersResponse>, { unimplemented!() });

try_from!(item: &protowire::GetUtxosByAddressesRequestMessage, rpc_core::GetUtxosByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetUtxosByAddressesResponseMessage, RpcResult<rpc_core::GetUtxosByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(_item: &protowire::GetBalanceByAddressRequestMessage, rpc_core::GetBalanceByAddressRequest, { unimplemented!() });
try_from!(_item: &protowire::GetBalanceByAddressResponseMessage, RpcResult<rpc_core::GetBalanceByAddressResponse>, {
    unimplemented!()
});

try_from!(_item: &protowire::GetBalancesByAddressesRequestMessage, rpc_core::GetBalancesByAddressesRequest, { unimplemented!() });
try_from!(_item: &protowire::GetBalancesByAddressesResponseMessage, RpcResult<rpc_core::GetBalancesByAddressesResponse>, {
    unimplemented!()
});

try_from!(&protowire::GetVirtualSelectedParentBlueScoreRequestMessage, rpc_core::GetVirtualSelectedParentBlueScoreRequest);
try_from!(
    item: &protowire::GetVirtualSelectedParentBlueScoreResponseMessage,
    RpcResult<rpc_core::GetVirtualSelectedParentBlueScoreResponse>,
    { Self { blue_score: item.blue_score } }
);

try_from!(_item: &protowire::BanRequestMessage, rpc_core::BanRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::BanResponseMessage, RpcResult<rpc_core::BanResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::UnbanRequestMessage, rpc_core::UnbanRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::UnbanResponseMessage, RpcResult<rpc_core::UnbanResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::EstimateNetworkHashesPerSecondRequestMessage, rpc_core::EstimateNetworkHashesPerSecondRequest, {
    unimplemented!()
});
try_from!(
    _item: &protowire::EstimateNetworkHashesPerSecondResponseMessage,
    RpcResult<rpc_core::EstimateNetworkHashesPerSecondResponse>,
    { unimplemented!() }
);

try_from!(_item: &protowire::GetMempoolEntriesByAddressesRequestMessage, rpc_core::GetMempoolEntriesByAddressesRequest, {
    unimplemented!()
});
try_from!(
    _item: &protowire::GetMempoolEntriesByAddressesResponseMessage,
    RpcResult<rpc_core::GetMempoolEntriesByAddressesResponse>,
    { unimplemented!() }
);

try_from!(_item: &protowire::GetCoinSupplyRequestMessage, rpc_core::GetCoinSupplyRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetCoinSupplyResponseMessage, RpcResult<rpc_core::GetCoinSupplyResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::PingRequestMessage, rpc_core::PingRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::PingResponseMessage, RpcResult<rpc_core::PingResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetProcessMetricsRequestMessage, rpc_core::GetProcessMetricsRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetProcessMetricsResponseMessage, RpcResult<rpc_core::GetProcessMetricsResponse>, {
    //
    unimplemented!()
});

try_from!(item: &protowire::NotifyUtxosChangedRequestMessage, rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: item.command.into(),
    }
});
try_from!(&protowire::NotifyUtxosChangedResponseMessage, RpcResult<rpc_core::NotifyUtxosChangedResponse>);

try_from!(item: &protowire::NotifyPruningPointUtxoSetOverrideRequestMessage, rpc_core::NotifyPruningPointUtxoSetOverrideRequest, {
    Self { command: item.command.into() }
});
try_from!(
    &protowire::NotifyPruningPointUtxoSetOverrideResponseMessage,
    RpcResult<rpc_core::NotifyPruningPointUtxoSetOverrideResponse>
);

try_from!(item: &protowire::NotifyFinalityConflictRequestMessage, rpc_core::NotifyFinalityConflictRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyFinalityConflictResponseMessage, RpcResult<rpc_core::NotifyFinalityConflictResponse>);

try_from!(item: &protowire::NotifyVirtualDaaScoreChangedRequestMessage, rpc_core::NotifyVirtualDaaScoreChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyVirtualDaaScoreChangedResponseMessage, RpcResult<rpc_core::NotifyVirtualDaaScoreChangedResponse>);

try_from!(
    item: &protowire::NotifyVirtualSelectedParentChainChangedRequestMessage,
    rpc_core::NotifyVirtualSelectedParentChainChangedRequest,
    { Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() } }
);
try_from!(
    &protowire::NotifyVirtualSelectedParentChainChangedResponseMessage,
    RpcResult<rpc_core::NotifyVirtualSelectedParentChainChangedResponse>
);

try_from!(
    item: &protowire::NotifyVirtualSelectedParentBlueScoreChangedRequestMessage,
    rpc_core::NotifyVirtualSelectedParentBlueScoreChangedRequest,
    { Self { command: item.command.into() } }
);
try_from!(
    &protowire::NotifyVirtualSelectedParentBlueScoreChangedResponseMessage,
    RpcResult<rpc_core::NotifyVirtualSelectedParentBlueScoreChangedResponse>
);

// ----------------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------------

// TODO: tests
