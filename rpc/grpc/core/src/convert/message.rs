use crate::protowire::{self, submit_block_response_message::RejectReason};
use kaspa_rpc_core::{RpcError, RpcExtraData, RpcHash, RpcResult};
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

from!(item: &kaspa_rpc_core::SubmitBlockReport, RejectReason, {
    match item {
        kaspa_rpc_core::SubmitBlockReport::Success => RejectReason::None,
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
    }
});

from!(item: &kaspa_rpc_core::SubmitBlockRequest, protowire::SubmitBlockRequestMessage, {
    Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitBlockResponse>, protowire::SubmitBlockResponseMessage, {
    Self { reject_reason: RejectReason::from(&item.report) as i32, error: None }
});

from!(item: &kaspa_rpc_core::GetBlockTemplateRequest, protowire::GetBlockTemplateRequestMessage, {
    Self {
        pay_address: (&item.pay_address).into(),
        extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlockTemplateResponse>, protowire::GetBlockTemplateResponseMessage, {
    Self { block: Some((&item.block).into()), is_synced: item.is_synced, error: None }
});

from!(item: &kaspa_rpc_core::GetBlockRequest, protowire::GetBlockRequestMessage, {
    Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlockResponse>, protowire::GetBlockResponseMessage, {
    Self { block: Some((&item.block).into()), error: None }
});

from!(item: &kaspa_rpc_core::NotifyBlockAddedRequest, protowire::NotifyBlockAddedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyBlockAddedResponse>, protowire::NotifyBlockAddedResponseMessage);

from!(&kaspa_rpc_core::GetInfoRequest, protowire::GetInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetInfoResponse>, protowire::GetInfoResponseMessage, {
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

from!(item: &kaspa_rpc_core::NotifyNewBlockTemplateRequest, protowire::NotifyNewBlockTemplateRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyNewBlockTemplateResponse>, protowire::NotifyNewBlockTemplateResponseMessage);

// ~~~

from!(&kaspa_rpc_core::GetCurrentNetworkRequest, protowire::GetCurrentNetworkRequestMessage);
from!(_item: RpcResult<&kaspa_rpc_core::GetCurrentNetworkResponse>, protowire::GetCurrentNetworkResponseMessage, {
    unimplemented!();
});

from!(&kaspa_rpc_core::GetPeerAddressesRequest, protowire::GetPeerAddressesRequestMessage);
from!(_item: RpcResult<&kaspa_rpc_core::GetPeerAddressesResponse>, protowire::GetPeerAddressesResponseMessage, {
    unimplemented!();
});

from!(&kaspa_rpc_core::GetSelectedTipHashRequest, protowire::GetSelectedTipHashRequestMessage);
from!(_item: RpcResult<&kaspa_rpc_core::GetSelectedTipHashResponse>, protowire::GetSelectedTipHashResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetMempoolEntryRequest, protowire::GetMempoolEntryRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&kaspa_rpc_core::GetMempoolEntryResponse>, protowire::GetMempoolEntryResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetMempoolEntriesRequest, protowire::GetMempoolEntriesRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesResponse>, protowire::GetMempoolEntriesResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetConnectedPeerInfoRequest, protowire::GetConnectedPeerInfoRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&kaspa_rpc_core::GetConnectedPeerInfoResponse>, protowire::GetConnectedPeerInfoResponseMessage, {
    unimplemented!()
});

from!(_item: &kaspa_rpc_core::AddPeerRequest, protowire::AddPeerRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::AddPeerResponse>, protowire::AddPeerResponseMessage, {
    unimplemented!();
});

from!(item: &kaspa_rpc_core::SubmitTransactionRequest, protowire::SubmitTransactionRequestMessage, {
    Self { transaction: Some((&item.transaction).into()), allow_orphan: item.allow_orphan }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitTransactionResponse>, protowire::SubmitTransactionResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), error: None }
});

from!(_item: &kaspa_rpc_core::GetSubnetworkRequest, protowire::GetSubnetworkRequestMessage, { unimplemented!() });
from!(_item: RpcResult<&kaspa_rpc_core::GetSubnetworkResponse>, protowire::GetSubnetworkResponseMessage, {
    unimplemented!();
});

// ~~~

from!(_item: &kaspa_rpc_core::GetVirtualChainFromBlockRequest, protowire::GetVirtualChainFromBlockRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetVirtualChainFromBlockResponse>, protowire::GetVirtualChainFromBlockResponseMessage, {
    unimplemented!();
});

from!(item: &kaspa_rpc_core::GetBlocksRequest, protowire::GetBlocksRequestMessage, {
    Self { low_hash: item.low_hash.to_string(), include_blocks: item.include_blocks, include_transactions: item.include_transactions }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlocksResponse>, protowire::GetBlocksResponseMessage, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        blocks: item.blocks.iter().map(|x| x.into()).collect::<Vec<_>>(),
        error: None,
    }
});

from!(_item: &kaspa_rpc_core::GetBlockCountRequest, protowire::GetBlockCountRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetBlockCountResponse>, protowire::GetBlockCountResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetBlockDagInfoRequest, protowire::GetBlockDagInfoRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetBlockDagInfoResponse>, protowire::GetBlockDagInfoResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::ResolveFinalityConflictRequest, protowire::ResolveFinalityConflictRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::ResolveFinalityConflictResponse>, protowire::ResolveFinalityConflictResponseMessage, {
    unimplemented!();
});

from!(&kaspa_rpc_core::ShutdownRequest, protowire::ShutdownRequestMessage);
from!(RpcResult<&kaspa_rpc_core::ShutdownResponse>, protowire::ShutdownResponseMessage);

from!(_item: &kaspa_rpc_core::GetHeadersRequest, protowire::GetHeadersRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetHeadersResponse>, protowire::GetHeadersResponseMessage, {
    unimplemented!();
});

from!(item: &kaspa_rpc_core::GetUtxosByAddressesRequest, protowire::GetUtxosByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetUtxosByAddressesResponse>, protowire::GetUtxosByAddressesResponseMessage, {
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(_item: &kaspa_rpc_core::GetBalanceByAddressRequest, protowire::GetBalanceByAddressRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetBalanceByAddressResponse>, protowire::GetBalanceByAddressResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetBalancesByAddressesRequest, protowire::GetBalancesByAddressesRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetBalancesByAddressesResponse>, protowire::GetBalancesByAddressesResponseMessage, {
    unimplemented!();
});

from!(&kaspa_rpc_core::GetVirtualSelectedParentBlueScoreRequest, protowire::GetVirtualSelectedParentBlueScoreRequestMessage);
from!(
    item: RpcResult<&kaspa_rpc_core::GetVirtualSelectedParentBlueScoreResponse>,
    protowire::GetVirtualSelectedParentBlueScoreResponseMessage,
    { Self { blue_score: item.blue_score, error: None } }
);

from!(_item: &kaspa_rpc_core::BanRequest, protowire::BanRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::BanResponse>, protowire::BanResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::UnbanRequest, protowire::UnbanRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::UnbanResponse>, protowire::UnbanResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, protowire::EstimateNetworkHashesPerSecondRequestMessage, {
    unimplemented!();
});
from!(
    _item: RpcResult<&kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    protowire::EstimateNetworkHashesPerSecondResponseMessage,
    {
        unimplemented!();
    }
);

from!(_item: &kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, protowire::GetMempoolEntriesByAddressesRequestMessage, {
    unimplemented!();
});
from!(
    _item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    protowire::GetMempoolEntriesByAddressesResponseMessage,
    {
        unimplemented!();
    }
);

from!(_item: &kaspa_rpc_core::GetCoinSupplyRequest, protowire::GetCoinSupplyRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetCoinSupplyResponse>, protowire::GetCoinSupplyResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::PingRequest, protowire::PingRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::PingResponse>, protowire::PingResponseMessage, {
    unimplemented!();
});

from!(_item: &kaspa_rpc_core::GetProcessMetricsRequest, protowire::GetProcessMetricsRequestMessage, {
    unimplemented!();
});
from!(_item: RpcResult<&kaspa_rpc_core::GetProcessMetricsResponse>, protowire::GetProcessMetricsResponseMessage, {
    unimplemented!();
});

from!(item: &kaspa_rpc_core::NotifyUtxosChangedRequest, protowire::NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect(), command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyUtxosChangedResponse>, protowire::NotifyUtxosChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: item.command.into() }
});
from!(
    RpcResult<&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>,
    protowire::NotifyPruningPointUtxoSetOverrideResponseMessage
);

from!(item: &kaspa_rpc_core::NotifyFinalityConflictRequest, protowire::NotifyFinalityConflictRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyFinalityConflictResponse>, protowire::NotifyFinalityConflictResponseMessage);

from!(item: &kaspa_rpc_core::NotifyVirtualDaaScoreChangedRequest, protowire::NotifyVirtualDaaScoreChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyVirtualDaaScoreChangedResponse>, protowire::NotifyVirtualDaaScoreChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifyVirtualChainChangedRequest, protowire::NotifyVirtualChainChangedRequestMessage, {
    Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyVirtualChainChangedResponse>, protowire::NotifyVirtualChainChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifySinkBlueScoreChangedRequest, protowire::NotifySinkBlueScoreChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifySinkBlueScoreChangedResponse>, protowire::NotifySinkBlueScoreChangedResponseMessage);

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

from!(item: RejectReason, kaspa_rpc_core::SubmitBlockReport, {
    match item {
        RejectReason::None => kaspa_rpc_core::SubmitBlockReport::Success,
        RejectReason::BlockInvalid => kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::BlockInvalid),
        RejectReason::IsInIbd => kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::IsInIBD),
    }
});

try_from!(item: &protowire::SubmitBlockRequestMessage, kaspa_rpc_core::SubmitBlockRequest, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()))?
            .try_into()?,
        allow_non_daa_blocks: item.allow_non_daa_blocks,
    }
});
try_from!(item: &protowire::SubmitBlockResponseMessage, RpcResult<kaspa_rpc_core::SubmitBlockResponse>, {
    Self { report: RejectReason::from_i32(item.reject_reason).ok_or(RpcError::PrimitiveToEnumConversionError)?.into() }
});

try_from!(item: &protowire::GetBlockTemplateRequestMessage, kaspa_rpc_core::GetBlockTemplateRequest, {
    Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) }
});
try_from!(item: &protowire::GetBlockTemplateResponseMessage, RpcResult<kaspa_rpc_core::GetBlockTemplateResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockTemplateResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
        is_synced: item.is_synced,
    }
});

try_from!(item: &protowire::GetBlockRequestMessage, kaspa_rpc_core::GetBlockRequest, {
    Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions }
});
try_from!(item: &protowire::GetBlockResponseMessage, RpcResult<kaspa_rpc_core::GetBlockResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::NotifyBlockAddedRequestMessage, kaspa_rpc_core::NotifyBlockAddedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyBlockAddedResponseMessage, RpcResult<kaspa_rpc_core::NotifyBlockAddedResponse>);

try_from!(&protowire::GetInfoRequestMessage, kaspa_rpc_core::GetInfoRequest);
try_from!(item: &protowire::GetInfoResponseMessage, RpcResult<kaspa_rpc_core::GetInfoResponse>, {
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

try_from!(item: &protowire::NotifyNewBlockTemplateRequestMessage, kaspa_rpc_core::NotifyNewBlockTemplateRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyNewBlockTemplateResponseMessage, RpcResult<kaspa_rpc_core::NotifyNewBlockTemplateResponse>);

// ~~~

try_from!(_item: &protowire::GetCurrentNetworkRequestMessage, kaspa_rpc_core::GetCurrentNetworkRequest, { unimplemented!() });
try_from!(_item: &protowire::GetCurrentNetworkResponseMessage, RpcResult<kaspa_rpc_core::GetCurrentNetworkResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetPeerAddressesRequestMessage, kaspa_rpc_core::GetPeerAddressesRequest, { unimplemented!() });
try_from!(_item: &protowire::GetPeerAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetPeerAddressesResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetSelectedTipHashRequestMessage, kaspa_rpc_core::GetSelectedTipHashRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetSelectedTipHashResponseMessage, RpcResult<kaspa_rpc_core::GetSelectedTipHashResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntryRequestMessage, kaspa_rpc_core::GetMempoolEntryRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetMempoolEntryResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntryResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetMempoolEntriesRequestMessage, kaspa_rpc_core::GetMempoolEntriesRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetMempoolEntriesResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntriesResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetConnectedPeerInfoRequestMessage, kaspa_rpc_core::GetConnectedPeerInfoRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetConnectedPeerInfoResponseMessage, RpcResult<kaspa_rpc_core::GetConnectedPeerInfoResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::AddPeerRequestMessage, kaspa_rpc_core::AddPeerRequest, {
    //
    unimplemented!()
});
try_from!(item: &protowire::AddPeerResponseMessage, RpcResult<kaspa_rpc_core::AddPeerResponse>, { unimplemented!() });

try_from!(item: &protowire::SubmitTransactionRequestMessage, kaspa_rpc_core::SubmitTransactionRequest, {
    Self {
        transaction: item
            .transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionRequestMessage".to_string(), "transaction".to_string()))?
            .try_into()?,
        allow_orphan: item.allow_orphan,
    }
});
try_from!(item: &protowire::SubmitTransactionResponseMessage, RpcResult<kaspa_rpc_core::SubmitTransactionResponse>, {
    Self { transaction_id: RpcHash::from_str(&item.transaction_id)? }
});

try_from!(_item: &protowire::GetSubnetworkRequestMessage, kaspa_rpc_core::GetSubnetworkRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetSubnetworkResponseMessage, RpcResult<kaspa_rpc_core::GetSubnetworkResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetVirtualChainFromBlockRequestMessage, kaspa_rpc_core::GetVirtualChainFromBlockRequest, {
    unimplemented!()
});
try_from!(_item: &protowire::GetVirtualChainFromBlockResponseMessage, RpcResult<kaspa_rpc_core::GetVirtualChainFromBlockResponse>, {
    unimplemented!()
});

try_from!(item: &protowire::GetBlocksRequestMessage, kaspa_rpc_core::GetBlocksRequest, {
    Self {
        low_hash: RpcHash::from_str(&item.low_hash)?,
        include_blocks: item.include_blocks,
        include_transactions: item.include_transactions,
    }
});
try_from!(item: &protowire::GetBlocksResponseMessage, RpcResult<kaspa_rpc_core::GetBlocksResponse>, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        blocks: item.blocks.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(_item: &protowire::GetBlockCountRequestMessage, kaspa_rpc_core::GetBlockCountRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetBlockCountResponseMessage, RpcResult<kaspa_rpc_core::GetBlockCountResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetBlockDagInfoRequestMessage, kaspa_rpc_core::GetBlockDagInfoRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetBlockDagInfoResponseMessage, RpcResult<kaspa_rpc_core::GetBlockDagInfoResponse>, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::ResolveFinalityConflictRequestMessage, kaspa_rpc_core::ResolveFinalityConflictRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::ResolveFinalityConflictResponseMessage, RpcResult<kaspa_rpc_core::ResolveFinalityConflictResponse>, {
    unimplemented!()
});

try_from!(&protowire::ShutdownRequestMessage, kaspa_rpc_core::ShutdownRequest);
try_from!(&protowire::ShutdownResponseMessage, RpcResult<kaspa_rpc_core::ShutdownResponse>);

try_from!(_item: &protowire::GetHeadersRequestMessage, kaspa_rpc_core::GetHeadersRequest, { unimplemented!() });
try_from!(_item: &protowire::GetHeadersResponseMessage, RpcResult<kaspa_rpc_core::GetHeadersResponse>, { unimplemented!() });

try_from!(item: &protowire::GetUtxosByAddressesRequestMessage, kaspa_rpc_core::GetUtxosByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetUtxosByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetUtxosByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(_item: &protowire::GetBalanceByAddressRequestMessage, kaspa_rpc_core::GetBalanceByAddressRequest, { unimplemented!() });
try_from!(_item: &protowire::GetBalanceByAddressResponseMessage, RpcResult<kaspa_rpc_core::GetBalanceByAddressResponse>, {
    unimplemented!()
});

try_from!(_item: &protowire::GetBalancesByAddressesRequestMessage, kaspa_rpc_core::GetBalancesByAddressesRequest, {
    unimplemented!()
});
try_from!(_item: &protowire::GetBalancesByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetBalancesByAddressesResponse>, {
    unimplemented!()
});

try_from!(&protowire::GetVirtualSelectedParentBlueScoreRequestMessage, kaspa_rpc_core::GetVirtualSelectedParentBlueScoreRequest);
try_from!(
    item: &protowire::GetVirtualSelectedParentBlueScoreResponseMessage,
    RpcResult<kaspa_rpc_core::GetVirtualSelectedParentBlueScoreResponse>,
    { Self { blue_score: item.blue_score } }
);

try_from!(_item: &protowire::BanRequestMessage, kaspa_rpc_core::BanRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::BanResponseMessage, RpcResult<kaspa_rpc_core::BanResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::UnbanRequestMessage, kaspa_rpc_core::UnbanRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::UnbanResponseMessage, RpcResult<kaspa_rpc_core::UnbanResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::EstimateNetworkHashesPerSecondRequestMessage, kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, {
    unimplemented!()
});
try_from!(
    _item: &protowire::EstimateNetworkHashesPerSecondResponseMessage,
    RpcResult<kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    { unimplemented!() }
);

try_from!(_item: &protowire::GetMempoolEntriesByAddressesRequestMessage, kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, {
    unimplemented!()
});
try_from!(
    _item: &protowire::GetMempoolEntriesByAddressesResponseMessage,
    RpcResult<kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    { unimplemented!() }
);

try_from!(_item: &protowire::GetCoinSupplyRequestMessage, kaspa_rpc_core::GetCoinSupplyRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetCoinSupplyResponseMessage, RpcResult<kaspa_rpc_core::GetCoinSupplyResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::PingRequestMessage, kaspa_rpc_core::PingRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::PingResponseMessage, RpcResult<kaspa_rpc_core::PingResponse>, {
    //
    unimplemented!()
});

try_from!(_item: &protowire::GetProcessMetricsRequestMessage, kaspa_rpc_core::GetProcessMetricsRequest, {
    //
    unimplemented!()
});
try_from!(_item: &protowire::GetProcessMetricsResponseMessage, RpcResult<kaspa_rpc_core::GetProcessMetricsResponse>, {
    //
    unimplemented!()
});

try_from!(item: &protowire::NotifyUtxosChangedRequestMessage, kaspa_rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: item.command.into(),
    }
});
try_from!(&protowire::NotifyUtxosChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyUtxosChangedResponse>);

try_from!(
    item: &protowire::NotifyPruningPointUtxoSetOverrideRequestMessage,
    kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest,
    { Self { command: item.command.into() } }
);
try_from!(
    &protowire::NotifyPruningPointUtxoSetOverrideResponseMessage,
    RpcResult<kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>
);

try_from!(item: &protowire::NotifyFinalityConflictRequestMessage, kaspa_rpc_core::NotifyFinalityConflictRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyFinalityConflictResponseMessage, RpcResult<kaspa_rpc_core::NotifyFinalityConflictResponse>);

try_from!(item: &protowire::NotifyVirtualDaaScoreChangedRequestMessage, kaspa_rpc_core::NotifyVirtualDaaScoreChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyVirtualDaaScoreChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyVirtualDaaScoreChangedResponse>);

try_from!(item: &protowire::NotifyVirtualChainChangedRequestMessage, kaspa_rpc_core::NotifyVirtualChainChangedRequest, {
    Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() }
});
try_from!(&protowire::NotifyVirtualChainChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyVirtualChainChangedResponse>);

try_from!(item: &protowire::NotifySinkBlueScoreChangedRequestMessage, kaspa_rpc_core::NotifySinkBlueScoreChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifySinkBlueScoreChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifySinkBlueScoreChangedResponse>);

// ----------------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------------

// TODO: tests
