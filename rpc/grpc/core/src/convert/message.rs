use crate::protowire::{self, submit_block_response_message::RejectReason};
use kaspa_notify::subscription::Command;
use kaspa_rpc_core::{
    RpcContextualPeerAddress, RpcError, RpcExtraData, RpcHash, RpcIpAddress, RpcNetworkType, RpcPeerAddress, RpcResult,
};
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
from!(item: RpcResult<&kaspa_rpc_core::GetCurrentNetworkResponse>, protowire::GetCurrentNetworkResponseMessage, {
    Self { current_network: item.network.to_string(), error: None }
});

from!(&kaspa_rpc_core::GetPeerAddressesRequest, protowire::GetPeerAddressesRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetPeerAddressesResponse>, protowire::GetPeerAddressesResponseMessage, {
    Self {
        addresses: item.known_addresses.iter().map(|x| x.into()).collect(),
        banned_addresses: item.banned_addresses.iter().map(|x| x.into()).collect(),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSelectedTipHashRequest, protowire::GetSelectedTipHashRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSelectedTipHashResponse>, protowire::GetSelectedTipHashResponseMessage, {
    Self { selected_tip_hash: item.selected_tip_hash.to_string(), error: None }
});

from!(item: &kaspa_rpc_core::GetMempoolEntryRequest, protowire::GetMempoolEntryRequestMessage, {
    Self {
        tx_id: item.transaction_id.to_string(),
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMempoolEntryResponse>, protowire::GetMempoolEntryResponseMessage, {
    Self { entry: Some((&item.mempool_entry).into()), error: None }
});

from!(item: &kaspa_rpc_core::GetMempoolEntriesRequest, protowire::GetMempoolEntriesRequestMessage, {
    Self { include_orphan_pool: item.include_orphan_pool, filter_transaction_pool: item.filter_transaction_pool }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesResponse>, protowire::GetMempoolEntriesResponseMessage, {
    Self { entries: item.mempool_entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(&kaspa_rpc_core::GetConnectedPeerInfoRequest, protowire::GetConnectedPeerInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetConnectedPeerInfoResponse>, protowire::GetConnectedPeerInfoResponseMessage, {
    Self { infos: item.peer_info.iter().map(|x| x.into()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::AddPeerRequest, protowire::AddPeerRequestMessage, {
    Self { address: item.peer_address.to_string(), is_permanent: item.is_permanent }
});
from!(RpcResult<&kaspa_rpc_core::AddPeerResponse>, protowire::AddPeerResponseMessage);

from!(item: &kaspa_rpc_core::SubmitTransactionRequest, protowire::SubmitTransactionRequestMessage, {
    Self { transaction: Some((&item.transaction).into()), allow_orphan: item.allow_orphan }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitTransactionResponse>, protowire::SubmitTransactionResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), error: None }
});

from!(item: &kaspa_rpc_core::GetSubnetworkRequest, protowire::GetSubnetworkRequestMessage, {
    Self { subnetwork_id: item.subnetwork_id.to_string() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetSubnetworkResponse>, protowire::GetSubnetworkResponseMessage, {
    Self { gas_limit: item.gas_limit, error: None }
});

// ~~~

from!(item: &kaspa_rpc_core::GetVirtualChainFromBlockRequest, protowire::GetVirtualChainFromBlockRequestMessage, {
    Self { start_hash: item.start_hash.to_string(), include_accepted_transaction_ids: item.include_accepted_transaction_ids }
});
from!(item: RpcResult<&kaspa_rpc_core::GetVirtualChainFromBlockResponse>, protowire::GetVirtualChainFromBlockResponseMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.into()).collect(),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::GetBlocksRequest, protowire::GetBlocksRequestMessage, {
    Self {
        low_hash: item.low_hash.map_or(Default::default(), |x| x.to_string()),
        include_blocks: item.include_blocks,
        include_transactions: item.include_transactions,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlocksResponse>, protowire::GetBlocksResponseMessage, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        blocks: item.blocks.iter().map(|x| x.into()).collect::<Vec<_>>(),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetBlockCountRequest, protowire::GetBlockCountRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetBlockCountResponse>, protowire::GetBlockCountResponseMessage, {
    Self { block_count: item.block_count, header_count: item.header_count, error: None }
});

from!(&kaspa_rpc_core::GetBlockDagInfoRequest, protowire::GetBlockDagInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetBlockDagInfoResponse>, protowire::GetBlockDagInfoResponseMessage, {
    Self {
        network_name: item.network.to_prefixed(),
        block_count: item.block_count,
        header_count: item.header_count,
        tip_hashes: item.tip_hashes.iter().map(|x| x.to_string()).collect(),
        difficulty: item.difficulty,
        past_median_time: item.past_median_time as i64,
        virtual_parent_hashes: item.virtual_parent_hashes.iter().map(|x| x.to_string()).collect(),
        pruning_point_hash: item.pruning_point_hash.to_string(),
        virtual_daa_score: item.virtual_daa_score,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::ResolveFinalityConflictRequest, protowire::ResolveFinalityConflictRequestMessage, {
    Self { finality_block_hash: item.finality_block_hash.to_string() }
});
from!(_item: RpcResult<&kaspa_rpc_core::ResolveFinalityConflictResponse>, protowire::ResolveFinalityConflictResponseMessage, {
    Self { error: None }
});

from!(&kaspa_rpc_core::ShutdownRequest, protowire::ShutdownRequestMessage);
from!(RpcResult<&kaspa_rpc_core::ShutdownResponse>, protowire::ShutdownResponseMessage);

from!(item: &kaspa_rpc_core::GetHeadersRequest, protowire::GetHeadersRequestMessage, {
    Self { start_hash: item.start_hash.to_string(), limit: item.limit, is_ascending: item.is_ascending }
});
from!(item: RpcResult<&kaspa_rpc_core::GetHeadersResponse>, protowire::GetHeadersResponseMessage, {
    Self { headers: item.headers.iter().map(|x| x.hash.to_string()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::GetUtxosByAddressesRequest, protowire::GetUtxosByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetUtxosByAddressesResponse>, protowire::GetUtxosByAddressesResponseMessage, {
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::GetBalanceByAddressRequest, protowire::GetBalanceByAddressRequestMessage, {
    Self { address: (&item.address).into() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalanceByAddressResponse>, protowire::GetBalanceByAddressResponseMessage, {
    Self { balance: item.balance, error: None }
});

from!(item: &kaspa_rpc_core::GetBalancesByAddressesRequest, protowire::GetBalancesByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalancesByAddressesResponse>, protowire::GetBalancesByAddressesResponseMessage, {
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(&kaspa_rpc_core::GetSinkBlueScoreRequest, protowire::GetSinkBlueScoreRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSinkBlueScoreResponse>, protowire::GetSinkBlueScoreResponseMessage, {
    Self { blue_score: item.blue_score, error: None }
});

from!(item: &kaspa_rpc_core::BanRequest, protowire::BanRequestMessage, { Self { ip: item.ip.to_string() } });
from!(_item: RpcResult<&kaspa_rpc_core::BanResponse>, protowire::BanResponseMessage, { Self { error: None } });

from!(item: &kaspa_rpc_core::UnbanRequest, protowire::UnbanRequestMessage, { Self { ip: item.ip.to_string() } });
from!(_item: RpcResult<&kaspa_rpc_core::UnbanResponse>, protowire::UnbanResponseMessage, { Self { error: None } });

from!(item: &kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, protowire::EstimateNetworkHashesPerSecondRequestMessage, {
    Self { window_size: item.window_size, start_hash: item.start_hash.map_or(Default::default(), |x| x.to_string()) }
});
from!(
    item: RpcResult<&kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    protowire::EstimateNetworkHashesPerSecondResponseMessage,
    { Self { network_hashes_per_second: item.network_hashes_per_second, error: None } }
);

from!(item: &kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, protowire::GetMempoolEntriesByAddressesRequestMessage, {
    Self {
        addresses: item.addresses.iter().map(|x| x.into()).collect(),
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
from!(
    item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    protowire::GetMempoolEntriesByAddressesResponseMessage,
    { Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None } }
);

from!(&kaspa_rpc_core::GetCoinSupplyRequest, protowire::GetCoinSupplyRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetCoinSupplyResponse>, protowire::GetCoinSupplyResponseMessage, {
    Self { max_sompi: item.max_sompi, circulating_sompi: item.circulating_sompi, error: None }
});

from!(&kaspa_rpc_core::PingRequest, protowire::PingRequestMessage);
from!(RpcResult<&kaspa_rpc_core::PingResponse>, protowire::PingResponseMessage);

from!(item: &kaspa_rpc_core::GetMetricsRequest, protowire::GetMetricsRequestMessage, {
    Self {
        process_metrics: item.process_metrics,
        consensus_metrics: item.consensus_metrics
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMetricsResponse>, protowire::GetMetricsResponseMessage, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.into()),
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.into()),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::NotifyUtxosChangedRequest, protowire::NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect(), command: item.command.into() }
});
from!(item: &kaspa_rpc_core::NotifyUtxosChangedRequest, protowire::StopNotifyingUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyUtxosChangedResponse>, protowire::NotifyUtxosChangedResponseMessage);
from!(RpcResult<&kaspa_rpc_core::NotifyUtxosChangedResponse>, protowire::StopNotifyingUtxosChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: item.command.into() }
});
from!(&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::StopNotifyingPruningPointUtxoSetOverrideRequestMessage);
from!(
    RpcResult<&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>,
    protowire::NotifyPruningPointUtxoSetOverrideResponseMessage
);
from!(
    RpcResult<&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>,
    protowire::StopNotifyingPruningPointUtxoSetOverrideResponseMessage
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
    Self { report: RejectReason::try_from(item.reject_reason).map_err(|_| RpcError::PrimitiveToEnumConversionError)?.into() }
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

try_from!(&protowire::GetCurrentNetworkRequestMessage, kaspa_rpc_core::GetCurrentNetworkRequest);
try_from!(item: &protowire::GetCurrentNetworkResponseMessage, RpcResult<kaspa_rpc_core::GetCurrentNetworkResponse>, {
    // Note that current_network is first converted to lowercase because the golang implementation
    // returns a "human readable" version with a capital first letter while the rusty version
    // is fully lowercase.
    Self { network: RpcNetworkType::from_str(&item.current_network.to_lowercase())? }
});

try_from!(&protowire::GetPeerAddressesRequestMessage, kaspa_rpc_core::GetPeerAddressesRequest);
try_from!(item: &protowire::GetPeerAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetPeerAddressesResponse>, {
    Self {
        known_addresses: item.addresses.iter().map(RpcPeerAddress::try_from).collect::<Result<Vec<_>, _>>()?,
        banned_addresses: item.banned_addresses.iter().map(RpcIpAddress::try_from).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(&protowire::GetSelectedTipHashRequestMessage, kaspa_rpc_core::GetSelectedTipHashRequest);
try_from!(item: &protowire::GetSelectedTipHashResponseMessage, RpcResult<kaspa_rpc_core::GetSelectedTipHashResponse>, {
    Self { selected_tip_hash: RpcHash::from_str(&item.selected_tip_hash)? }
});

try_from!(item: &protowire::GetMempoolEntryRequestMessage, kaspa_rpc_core::GetMempoolEntryRequest, {
    Self {
        transaction_id: kaspa_rpc_core::RpcTransactionId::from_str(&item.tx_id)?,
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
try_from!(item: &protowire::GetMempoolEntryResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntryResponse>, {
    Self {
        mempool_entry: item
            .entry
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetMempoolEntryResponseMessage".to_string(), "entry".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::GetMempoolEntriesRequestMessage, kaspa_rpc_core::GetMempoolEntriesRequest, {
    Self { include_orphan_pool: item.include_orphan_pool, filter_transaction_pool: item.filter_transaction_pool }
});
try_from!(item: &protowire::GetMempoolEntriesResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntriesResponse>, {
    Self { mempool_entries: item.entries.iter().map(kaspa_rpc_core::RpcMempoolEntry::try_from).collect::<Result<Vec<_>, _>>()? }
});

try_from!(&protowire::GetConnectedPeerInfoRequestMessage, kaspa_rpc_core::GetConnectedPeerInfoRequest);
try_from!(item: &protowire::GetConnectedPeerInfoResponseMessage, RpcResult<kaspa_rpc_core::GetConnectedPeerInfoResponse>, {
    Self { peer_info: item.infos.iter().map(kaspa_rpc_core::RpcPeerInfo::try_from).collect::<Result<Vec<_>, _>>()? }
});

try_from!(item: &protowire::AddPeerRequestMessage, kaspa_rpc_core::AddPeerRequest, {
    Self { peer_address: RpcContextualPeerAddress::from_str(&item.address)?, is_permanent: item.is_permanent }
});
try_from!(&protowire::AddPeerResponseMessage, RpcResult<kaspa_rpc_core::AddPeerResponse>);

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

try_from!(item: &protowire::GetSubnetworkRequestMessage, kaspa_rpc_core::GetSubnetworkRequest, {
    Self { subnetwork_id: kaspa_rpc_core::RpcSubnetworkId::from_str(&item.subnetwork_id)? }
});
try_from!(item: &protowire::GetSubnetworkResponseMessage, RpcResult<kaspa_rpc_core::GetSubnetworkResponse>, {
    Self { gas_limit: item.gas_limit }
});

try_from!(item: &protowire::GetVirtualChainFromBlockRequestMessage, kaspa_rpc_core::GetVirtualChainFromBlockRequest, {
    Self { start_hash: RpcHash::from_str(&item.start_hash)?, include_accepted_transaction_ids: item.include_accepted_transaction_ids }
});
try_from!(item: &protowire::GetVirtualChainFromBlockResponseMessage, RpcResult<kaspa_rpc_core::GetVirtualChainFromBlockResponse>, {
    Self {
        removed_chain_block_hashes: item
            .removed_chain_block_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<_>, _>>()?,
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::GetBlocksRequestMessage, kaspa_rpc_core::GetBlocksRequest, {
    Self {
        low_hash: if item.low_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.low_hash)?) },
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

try_from!(&protowire::GetBlockCountRequestMessage, kaspa_rpc_core::GetBlockCountRequest);
try_from!(item: &protowire::GetBlockCountResponseMessage, RpcResult<kaspa_rpc_core::GetBlockCountResponse>, {
    Self { header_count: item.header_count, block_count: item.block_count }
});

try_from!(&protowire::GetBlockDagInfoRequestMessage, kaspa_rpc_core::GetBlockDagInfoRequest);
try_from!(item: &protowire::GetBlockDagInfoResponseMessage, RpcResult<kaspa_rpc_core::GetBlockDagInfoResponse>, {
    Self {
        network: kaspa_rpc_core::RpcNetworkId::from_prefixed(&item.network_name)?,
        block_count: item.block_count,
        header_count: item.header_count,
        tip_hashes: item.tip_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        difficulty: item.difficulty,
        past_median_time: item.past_median_time as u64,
        virtual_parent_hashes: item.virtual_parent_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        pruning_point_hash: RpcHash::from_str(&item.pruning_point_hash)?,
        virtual_daa_score: item.virtual_daa_score,
    }
});

try_from!(item: &protowire::ResolveFinalityConflictRequestMessage, kaspa_rpc_core::ResolveFinalityConflictRequest, {
    Self { finality_block_hash: RpcHash::from_str(&item.finality_block_hash)? }
});
try_from!(&protowire::ResolveFinalityConflictResponseMessage, RpcResult<kaspa_rpc_core::ResolveFinalityConflictResponse>);

try_from!(&protowire::ShutdownRequestMessage, kaspa_rpc_core::ShutdownRequest);
try_from!(&protowire::ShutdownResponseMessage, RpcResult<kaspa_rpc_core::ShutdownResponse>);

try_from!(item: &protowire::GetHeadersRequestMessage, kaspa_rpc_core::GetHeadersRequest, {
    Self { start_hash: RpcHash::from_str(&item.start_hash)?, limit: item.limit, is_ascending: item.is_ascending }
});
try_from!(item: &protowire::GetHeadersResponseMessage, RpcResult<kaspa_rpc_core::GetHeadersResponse>, {
    // TODO
    Self { headers: vec![] }
});

try_from!(item: &protowire::GetUtxosByAddressesRequestMessage, kaspa_rpc_core::GetUtxosByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetUtxosByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetUtxosByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(item: &protowire::GetBalanceByAddressRequestMessage, kaspa_rpc_core::GetBalanceByAddressRequest, {
    Self { address: item.address.as_str().try_into()? }
});
try_from!(item: &protowire::GetBalanceByAddressResponseMessage, RpcResult<kaspa_rpc_core::GetBalanceByAddressResponse>, {
    Self { balance: item.balance }
});

try_from!(item: &protowire::GetBalancesByAddressesRequestMessage, kaspa_rpc_core::GetBalancesByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetBalancesByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetBalancesByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(&protowire::GetSinkBlueScoreRequestMessage, kaspa_rpc_core::GetSinkBlueScoreRequest);
try_from!(item: &protowire::GetSinkBlueScoreResponseMessage, RpcResult<kaspa_rpc_core::GetSinkBlueScoreResponse>, {
    Self { blue_score: item.blue_score }
});

try_from!(item: &protowire::BanRequestMessage, kaspa_rpc_core::BanRequest, { Self { ip: RpcIpAddress::from_str(&item.ip)? } });
try_from!(&protowire::BanResponseMessage, RpcResult<kaspa_rpc_core::BanResponse>);

try_from!(item: &protowire::UnbanRequestMessage, kaspa_rpc_core::UnbanRequest, { Self { ip: RpcIpAddress::from_str(&item.ip)? } });
try_from!(&protowire::UnbanResponseMessage, RpcResult<kaspa_rpc_core::UnbanResponse>);

try_from!(item: &protowire::EstimateNetworkHashesPerSecondRequestMessage, kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, {
    Self {
        window_size: item.window_size,
        start_hash: if item.start_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.start_hash)?) },
    }
});
try_from!(
    item: &protowire::EstimateNetworkHashesPerSecondResponseMessage,
    RpcResult<kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    { Self { network_hashes_per_second: item.network_hashes_per_second } }
);

try_from!(item: &protowire::GetMempoolEntriesByAddressesRequestMessage, kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
try_from!(
    item: &protowire::GetMempoolEntriesByAddressesResponseMessage,
    RpcResult<kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    { Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? } }
);

try_from!(&protowire::GetCoinSupplyRequestMessage, kaspa_rpc_core::GetCoinSupplyRequest);
try_from!(item: &protowire::GetCoinSupplyResponseMessage, RpcResult<kaspa_rpc_core::GetCoinSupplyResponse>, {
    Self { max_sompi: item.max_sompi, circulating_sompi: item.circulating_sompi }
});

try_from!(&protowire::PingRequestMessage, kaspa_rpc_core::PingRequest);
try_from!(&protowire::PingResponseMessage, RpcResult<kaspa_rpc_core::PingResponse>);

try_from!(item: &protowire::GetMetricsRequestMessage, kaspa_rpc_core::GetMetricsRequest, {
    Self { process_metrics: item.process_metrics, consensus_metrics: item.consensus_metrics }
});
try_from!(item: &protowire::GetMetricsResponseMessage, RpcResult<kaspa_rpc_core::GetMetricsResponse>, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.try_into()).transpose()?,
    }
});

try_from!(item: &protowire::NotifyUtxosChangedRequestMessage, kaspa_rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: item.command.into(),
    }
});
try_from!(item: &protowire::StopNotifyingUtxosChangedRequestMessage, kaspa_rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: Command::Stop,
    }
});
try_from!(&protowire::NotifyUtxosChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyUtxosChangedResponse>);
try_from!(&protowire::StopNotifyingUtxosChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyUtxosChangedResponse>);

try_from!(
    item: &protowire::NotifyPruningPointUtxoSetOverrideRequestMessage,
    kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest,
    { Self { command: item.command.into() } }
);
try_from!(
    _item: &protowire::StopNotifyingPruningPointUtxoSetOverrideRequestMessage,
    kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest,
    { Self { command: Command::Stop } }
);
try_from!(
    &protowire::NotifyPruningPointUtxoSetOverrideResponseMessage,
    RpcResult<kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>
);
try_from!(
    &protowire::StopNotifyingPruningPointUtxoSetOverrideResponseMessage,
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
