//! Conversions of protowire messages from and to rpc core counterparts.
//!
//! Response payloads in protowire do always contain an error field and generally a set of
//! fields providing the requested data.
//!
//! Responses in rpc core are expressed as RpcResult<XxxResponse>, where Xxx is the called
//! RPC method.
//!
//! The general conversion convention from protowire to rpc core is to consider the error
//! field first and, if present, to return a matching Err(RpcError). If absent, try to
//! convert the set of data fields into a matching XxxResponse rpc core response and, on
//! success, return Ok(XxxResponse), otherwise return a conversion error.
//!
//! Conversely, the general conversion convention from rpc core to protowire, depending on
//! a provided RpcResult is to either convert the Ok(XxxResponse) into the matching set
//! of data fields and provide no error or provide no data fields but an error field in case
//! of Err(RpcError).
//!
//! The SubmitBlockResponse is a notable exception to this general rule.

use crate::protowire::{self, submit_block_response_message::RejectReason};
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::debug;
use kaspa_notify::subscription::Command;
use kaspa_rpc_core::{
    RpcContextualPeerAddress, RpcError, RpcExtraData, RpcHash, RpcIpAddress, RpcNetworkType, RpcPeerAddress, RpcResult,
    SubmitBlockRejectReason, SubmitBlockReport,
};
use kaspa_utils::hex::*;
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
        // The conversion of RouteIsFull falls back to None since there exist no such variant in the original protowire version
        // and we do not want to break backwards compatibility
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::RouteIsFull) => RejectReason::None,
    }
});

from!(item: &kaspa_rpc_core::SubmitBlockRequest, protowire::SubmitBlockRequestMessage, {
    Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
});
// This conversion breaks the general conversion convention (see file header) since the message may
// contain both a non default reject_reason and a matching error message. In the RouteIsFull case
// reject_reason is None (because this reason has no variant in protowire) but a specific error
// message is provided.
from!(item: RpcResult<&kaspa_rpc_core::SubmitBlockResponse>, protowire::SubmitBlockResponseMessage, {
    let error: Option<protowire::RpcError> = match item.report {
        kaspa_rpc_core::SubmitBlockReport::Success => None,
        kaspa_rpc_core::SubmitBlockReport::Reject(reason) => Some(RpcError::SubmitBlockError(reason).into())
    };
    Self { reject_reason: RejectReason::from(&item.report) as i32, error }
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

from!(&kaspa_rpc_core::GetSinkRequest, protowire::GetSinkRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSinkResponse>, protowire::GetSinkResponseMessage, {
    Self { sink: item.sink.to_string(), error: None }
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

from!(item: &kaspa_rpc_core::SubmitTransactionReplacementRequest, protowire::SubmitTransactionReplacementRequestMessage, {
    Self { transaction: Some((&item.transaction).into()) }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitTransactionReplacementResponse>, protowire::SubmitTransactionReplacementResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), replaced_transaction: Some((&item.replaced_transaction).into()), error: None }
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
        sink: item.sink.to_string(),
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
    debug!("GRPC, Creating GetUtxosByAddresses message with {} entries", item.entries.len());
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::GetBalanceByAddressRequest, protowire::GetBalanceByAddressRequestMessage, {
    Self { address: (&item.address).into() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalanceByAddressResponse>, protowire::GetBalanceByAddressResponseMessage, {
    debug!("GRPC, Creating GetBalanceByAddress messages");
    Self { balance: item.balance, error: None }
});

from!(item: &kaspa_rpc_core::GetBalancesByAddressesRequest, protowire::GetBalancesByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalancesByAddressesResponse>, protowire::GetBalancesByAddressesResponseMessage, {
    debug!("GRPC, Creating GetUtxosByAddresses message with {} entries", item.entries.len());
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

from!(item: &kaspa_rpc_core::GetDaaScoreTimestampEstimateRequest, protowire::GetDaaScoreTimestampEstimateRequestMessage, {
    Self {
        daa_scores: item.daa_scores.clone()
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetDaaScoreTimestampEstimateResponse>, protowire::GetDaaScoreTimestampEstimateResponseMessage, {
    Self { timestamps: item.timestamps.clone(), error: None }
});

// Fee estimate API

from!(&kaspa_rpc_core::GetFeeEstimateRequest, protowire::GetFeeEstimateRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetFeeEstimateResponse>, protowire::GetFeeEstimateResponseMessage, {
    Self { estimate: Some((&item.estimate).into()), error: None }
});
from!(item: &kaspa_rpc_core::GetFeeEstimateExperimentalRequest, protowire::GetFeeEstimateExperimentalRequestMessage, {
    Self {
        verbose: item.verbose
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetFeeEstimateExperimentalResponse>, protowire::GetFeeEstimateExperimentalResponseMessage, {
    Self {
        estimate: Some((&item.estimate).into()),
        verbose: item.verbose.as_ref().map(|x| x.into()),
        error: None
    }
});

from!(item: &kaspa_rpc_core::GetCurrentBlockColorRequest, protowire::GetCurrentBlockColorRequestMessage, {
    Self {
        hash: item.hash.to_string()
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetCurrentBlockColorResponse>, protowire::GetCurrentBlockColorResponseMessage, {
    Self { blue: item.blue, error: None }
});

from!(&kaspa_rpc_core::PingRequest, protowire::PingRequestMessage);
from!(RpcResult<&kaspa_rpc_core::PingResponse>, protowire::PingResponseMessage);

from!(item: &kaspa_rpc_core::GetMetricsRequest, protowire::GetMetricsRequestMessage, {
    Self {
        process_metrics: item.process_metrics,
        connection_metrics: item.connection_metrics,
        bandwidth_metrics: item.bandwidth_metrics,
        consensus_metrics: item.consensus_metrics,
        storage_metrics: item.storage_metrics,
        custom_metrics: item.custom_metrics,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMetricsResponse>, protowire::GetMetricsResponseMessage, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.into()),
        connection_metrics: item.connection_metrics.as_ref().map(|x| x.into()),
        bandwidth_metrics: item.bandwidth_metrics.as_ref().map(|x| x.into()),
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.into()),
        storage_metrics: item.storage_metrics.as_ref().map(|x| x.into()),
        // TODO
        // custom_metrics : None,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::GetConnectionsRequest, protowire::GetConnectionsRequestMessage, {
    Self {
        include_profile_data : item.include_profile_data,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetConnectionsResponse>, protowire::GetConnectionsResponseMessage, {
    Self {
        clients: item.clients,
        peers: item.peers as u32,
        profile_data: item.profile_data.as_ref().map(|x| x.into()),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSystemInfoRequest, protowire::GetSystemInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSystemInfoResponse>, protowire::GetSystemInfoResponseMessage, {
    Self {
        version : item.version.clone(),
        system_id : item.system_id.as_ref().map(|system_id|system_id.to_hex()).unwrap_or_default(),
        git_hash : item.git_hash.as_ref().map(|git_hash|git_hash.to_hex()).unwrap_or_default(),
        total_memory : item.total_memory,
        core_num : item.cpu_physical_cores as u32,
        fd_limit : item.fd_limit,
        error: None,
    }
});

from!(&kaspa_rpc_core::GetServerInfoRequest, protowire::GetServerInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetServerInfoResponse>, protowire::GetServerInfoResponseMessage, {
    Self {
        rpc_api_version: item.rpc_api_version as u32,
        rpc_api_revision: item.rpc_api_revision as u32,
        server_version: item.server_version.clone(),
        network_id: item.network_id.to_string(),
        has_utxo_index: item.has_utxo_index,
        is_synced: item.is_synced,
        virtual_daa_score: item.virtual_daa_score,
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSyncStatusRequest, protowire::GetSyncStatusRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSyncStatusResponse>, protowire::GetSyncStatusResponseMessage, {
    Self {
        is_synced: item.is_synced,
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
impl TryFrom<&protowire::SubmitBlockResponseMessage> for kaspa_rpc_core::SubmitBlockResponse {
    type Error = RpcError;
    // This conversion breaks the general conversion convention (see file header) since the message may
    // contain both a non-None reject_reason and a matching error message. Things get even challenging
    // in the RouteIsFull case where reject_reason is None (because this reason has no variant in protowire)
    // but a specific error message is provided.
    fn try_from(item: &protowire::SubmitBlockResponseMessage) -> RpcResult<Self> {
        let report: SubmitBlockReport =
            RejectReason::try_from(item.reject_reason).map_err(|_| RpcError::PrimitiveToEnumConversionError)?.into();
        if let Some(ref err) = item.error {
            match report {
                SubmitBlockReport::Success => {
                    if err.message == RpcError::SubmitBlockError(SubmitBlockRejectReason::RouteIsFull).to_string() {
                        Ok(Self { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::RouteIsFull) })
                    } else {
                        Err(err.into())
                    }
                }
                SubmitBlockReport::Reject(_) => Ok(Self { report }),
            }
        } else {
            Ok(Self { report })
        }
    }
}

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

try_from!(&protowire::GetSinkRequestMessage, kaspa_rpc_core::GetSinkRequest);
try_from!(item: &protowire::GetSinkResponseMessage, RpcResult<kaspa_rpc_core::GetSinkResponse>, {
    Self { sink: RpcHash::from_str(&item.sink)? }
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

try_from!(item: &protowire::SubmitTransactionReplacementRequestMessage, kaspa_rpc_core::SubmitTransactionReplacementRequest, {
    Self {
        transaction: item
            .transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionReplacementRequestMessage".to_string(), "transaction".to_string()))?
            .try_into()?,
    }
});
try_from!(item: &protowire::SubmitTransactionReplacementResponseMessage, RpcResult<kaspa_rpc_core::SubmitTransactionReplacementResponse>, {
    Self {
        transaction_id: RpcHash::from_str(&item.transaction_id)?,
        replaced_transaction: item
            .replaced_transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionReplacementRequestMessage".to_string(), "replaced_transaction".to_string()))?
            .try_into()?,
    }
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
        sink: item.sink.parse()?,
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

try_from!(item: &protowire::GetDaaScoreTimestampEstimateRequestMessage, kaspa_rpc_core::GetDaaScoreTimestampEstimateRequest , {
    Self {
        daa_scores: item.daa_scores.clone()
    }
});
try_from!(item: &protowire::GetDaaScoreTimestampEstimateResponseMessage, RpcResult<kaspa_rpc_core::GetDaaScoreTimestampEstimateResponse>, {
    Self { timestamps: item.timestamps.clone() }
});

try_from!(&protowire::GetFeeEstimateRequestMessage, kaspa_rpc_core::GetFeeEstimateRequest);
try_from!(item: &protowire::GetFeeEstimateResponseMessage, RpcResult<kaspa_rpc_core::GetFeeEstimateResponse>, {
    Self {
        estimate: item.estimate
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetFeeEstimateResponseMessage".to_string(), "estimate".to_string()))?
            .try_into()?
    }
});
try_from!(item: &protowire::GetFeeEstimateExperimentalRequestMessage, kaspa_rpc_core::GetFeeEstimateExperimentalRequest, {
    Self {
        verbose: item.verbose
    }
});
try_from!(item: &protowire::GetFeeEstimateExperimentalResponseMessage, RpcResult<kaspa_rpc_core::GetFeeEstimateExperimentalResponse>, {
    Self {
        estimate: item.estimate
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetFeeEstimateExperimentalResponseMessage".to_string(), "estimate".to_string()))?
            .try_into()?,
        verbose: item.verbose.as_ref().map(|x| x.try_into()).transpose()?
    }
});

try_from!(item: &protowire::GetCurrentBlockColorRequestMessage, kaspa_rpc_core::GetCurrentBlockColorRequest, {
    Self {
        hash: RpcHash::from_str(&item.hash)?
    }
});
try_from!(item: &protowire::GetCurrentBlockColorResponseMessage, RpcResult<kaspa_rpc_core::GetCurrentBlockColorResponse>, {
    Self {
        blue: item.blue
    }
});

try_from!(&protowire::PingRequestMessage, kaspa_rpc_core::PingRequest);
try_from!(&protowire::PingResponseMessage, RpcResult<kaspa_rpc_core::PingResponse>);

try_from!(item: &protowire::GetMetricsRequestMessage, kaspa_rpc_core::GetMetricsRequest, {
    Self {
        process_metrics: item.process_metrics,
        connection_metrics: item.connection_metrics,
        bandwidth_metrics:item.bandwidth_metrics,
        consensus_metrics: item.consensus_metrics,
        storage_metrics: item.storage_metrics,
        custom_metrics : item.custom_metrics,
    }
});
try_from!(item: &protowire::GetMetricsResponseMessage, RpcResult<kaspa_rpc_core::GetMetricsResponse>, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        connection_metrics: item.connection_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        bandwidth_metrics: item.bandwidth_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        storage_metrics: item.storage_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        // TODO
        custom_metrics: None,
    }
});

try_from!(item: &protowire::GetConnectionsRequestMessage, kaspa_rpc_core::GetConnectionsRequest, {
    Self { include_profile_data : item.include_profile_data }
});
try_from!(item: &protowire::GetConnectionsResponseMessage, RpcResult<kaspa_rpc_core::GetConnectionsResponse>, {
    Self {
        clients: item.clients,
        peers: item.peers as u16,
        profile_data: item.profile_data.as_ref().map(|x| x.try_into()).transpose()?,
    }
});

try_from!(&protowire::GetSystemInfoRequestMessage, kaspa_rpc_core::GetSystemInfoRequest);
try_from!(item: &protowire::GetSystemInfoResponseMessage, RpcResult<kaspa_rpc_core::GetSystemInfoResponse>, {
    Self {
        version: item.version.clone(),
        system_id: (!item.system_id.is_empty()).then(|| FromHex::from_hex(&item.system_id)).transpose()?,
        git_hash: (!item.git_hash.is_empty()).then(|| FromHex::from_hex(&item.git_hash)).transpose()?,
        total_memory: item.total_memory,
        cpu_physical_cores: item.core_num as u16,
        fd_limit: item.fd_limit,
    }
});

try_from!(&protowire::GetServerInfoRequestMessage, kaspa_rpc_core::GetServerInfoRequest);
try_from!(item: &protowire::GetServerInfoResponseMessage, RpcResult<kaspa_rpc_core::GetServerInfoResponse>, {
    Self {
        rpc_api_version: item.rpc_api_version as u16,
        rpc_api_revision: item.rpc_api_revision as u16,
        server_version: item.server_version.clone(),
        network_id: NetworkId::from_str(&item.network_id)?,
        has_utxo_index: item.has_utxo_index,
        is_synced: item.is_synced,
        virtual_daa_score: item.virtual_daa_score,
    }
});

try_from!(&protowire::GetSyncStatusRequestMessage, kaspa_rpc_core::GetSyncStatusRequest);
try_from!(item: &protowire::GetSyncStatusResponseMessage, RpcResult<kaspa_rpc_core::GetSyncStatusResponse>, {
    Self {
        is_synced: item.is_synced,
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

#[cfg(test)]
mod tests {
    use kaspa_rpc_core::{RpcError, RpcResult, SubmitBlockRejectReason, SubmitBlockReport, SubmitBlockResponse};

    use crate::protowire::{self, submit_block_response_message::RejectReason, SubmitBlockResponseMessage};

    #[test]
    fn test_submit_block_response() {
        struct Test {
            rpc_core: RpcResult<kaspa_rpc_core::SubmitBlockResponse>,
            protowire: protowire::SubmitBlockResponseMessage,
        }
        impl Test {
            fn new(
                rpc_core: RpcResult<kaspa_rpc_core::SubmitBlockResponse>,
                protowire: protowire::SubmitBlockResponseMessage,
            ) -> Self {
                Self { rpc_core, protowire }
            }
        }
        let tests = vec![
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Success }),
                SubmitBlockResponseMessage { reject_reason: RejectReason::None as i32, error: None },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::BlockInvalid as i32,
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::BlockInvalid).to_string(),
                    }),
                },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::IsInIbd as i32,
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::IsInIBD).to_string(),
                    }),
                },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::RouteIsFull) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::None as i32, // This rpc core reject reason has no matching protowire variant
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::RouteIsFull).to_string(),
                    }),
                },
            ),
        ];

        for test in tests {
            let cnv_protowire: SubmitBlockResponseMessage = test.rpc_core.as_ref().map_err(|x| x.clone()).into();
            assert_eq!(cnv_protowire.reject_reason, test.protowire.reject_reason);
            assert_eq!(cnv_protowire.error.is_some(), test.protowire.error.is_some());
            assert_eq!(cnv_protowire.error, test.protowire.error);

            let cnv_rpc_core: RpcResult<SubmitBlockResponse> = (&test.protowire).try_into();
            assert_eq!(cnv_rpc_core.is_ok(), test.rpc_core.is_ok());
            match cnv_rpc_core {
                Ok(ref cnv_response) => {
                    let Ok(ref response) = test.rpc_core else { panic!() };
                    assert_eq!(cnv_response.report, response.report);
                }
                Err(ref cnv_err) => {
                    let Err(ref err) = test.rpc_core else { panic!() };
                    assert_eq!(cnv_err.to_string(), err.to_string());
                }
            }
        }
    }
}
