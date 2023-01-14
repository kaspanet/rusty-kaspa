// use std::fmt::{Display, Formatter};

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{api::ops::SubscribeCommand, model::*};

pub type RpcExtraData = Vec<u8>;

/// SubmitBlockRequest requests to submit a block into the DAG.
/// Blocks are generally expected to have been generated using the getBlockTemplate call.
///
/// See: [`GetBlockTemplateRequest`]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockRequest {
    pub block: RpcBlock,
    #[serde(alias = "allowNonDAABlocks")]
    pub allow_non_daa_blocks: bool,
}
impl SubmitBlockRequest {
    pub fn new(block: RpcBlock, allow_non_daa_blocks: bool) -> Self {
        Self { block, allow_non_daa_blocks }
    }
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub enum SubmitBlockRejectReason {
    // None = 0,
    BlockInvalid = 1,
    IsInIBD = 2,
}
// impl SubmitBlockRejectReason {
//     fn as_str(&self) -> &'static str {
//         // see app\appmessage\rpc_submit_block.go, line 35
//         match self {
//             SubmitBlockRejectReason::BlockInvalid => "Block is invalid",
//             SubmitBlockRejectReason::IsInIBD => "Node is in IBD",
//         }
//     }
// }
// impl Display for SubmitBlockRejectReason {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.write_str(self.as_str())
//     }
// }

// @tiram - wondering if this could be "leaner"
// albeit unlike Display this is not directly usable in format!()
impl ToString for SubmitBlockRejectReason {
    fn to_string(&self) -> String {
        match self {
            SubmitBlockRejectReason::BlockInvalid => "Block is invalid",
            SubmitBlockRejectReason::IsInIBD => "Node is in IBD",
        }
        .to_string()
    }
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub enum SubmitBlockReport {
    Success,
    Reject(SubmitBlockRejectReason),
}
impl SubmitBlockReport {
    pub fn is_success(&self) -> bool {
        *self == SubmitBlockReport::Success
    }
}

#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockResponse {
    pub report: SubmitBlockReport,
}

/// GetBlockTemplateRequest requests a current block template.
/// Callers are expected to solve the block template and submit it using the submitBlock call
///
/// See: [`SubmitBlockRequest`]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateRequest {
    /// Which kaspa address should the coinbase block reward transaction pay into
    pub pay_address: RpcAddress,
    pub extra_data: RpcExtraData,
}
impl GetBlockTemplateRequest {
    pub fn new(pay_address: RpcAddress, extra_data: RpcExtraData) -> Self {
        Self { pay_address, extra_data }
    }
}

#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateResponse {
    pub block: RpcBlock,

    /// Whether kaspad thinks that it's synced.
    /// Callers are discouraged (but not forbidden) from solving blocks when kaspad is not synced.
    /// That is because when kaspad isn't in sync with the rest of the network there's a high
    /// chance the block will never be accepted, thus the solving effort would have been wasted.
    pub is_synced: bool,
}
/// GetBlockRequest requests information about a specific block
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockRequest {
    /// The hash of the requested block
    pub hash: RpcHash,

    /// Whether to include transaction data in the response
    pub include_transactions: bool,
}
impl GetBlockRequest {
    pub fn new(hash: RpcHash, include_transactions: bool) -> Self {
        Self { hash, include_transactions }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockResponse {
    pub block: RpcBlock,
    // According to app\rpc\rpchandlers\get_block.go
    // block and error as mutually exclusive
}

/// NotifyBlockAddedRequest registers this connection for blockAdded notifications.
///
/// See: [`BlockAddedNotification`]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedRequest {
    pub command: SubscribeCommand,
}
impl NotifyBlockAddedRequest {
    pub fn new(command: SubscribeCommand) -> Self {
        Self { command }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedResponse {}

/// BlockAddedNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: [`NotifyBlockAddedRequest`]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockAddedNotification {
    pub block: RpcBlock,
}

/// GetInfoRequest returns info about the node.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoResponse {
    pub p2p_id: String,
    pub mempool_size: u64,
    pub server_version: String, // FIXME ?
    pub is_utxo_indexed: bool,
    pub is_synced: bool,
    pub has_notify_command: bool,
}

/// NotifyNewBlockTemplateRequest registers this connection for blockAdded notifications.
///
/// See: [`NewBlockTemplateNotification`]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateRequest {
    pub command: SubscribeCommand,
}
impl NotifyNewBlockTemplateRequest {
    pub fn new(command: SubscribeCommand) -> Self {
        Self { command }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateResponse {}

/// NewBlockTemplateNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: [`NotifyNewBlockTemplateRequest`]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewBlockTemplateNotification {}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkResponse {
    pub network: RpcNetworkType,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesResponse {
    pub known_addresses: Vec<RpcAddress>,
    pub banned_addresses: Vec<RpcAddress>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetSelectedTipHashRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetSelectedTipHashResponse {
    pub selected_tip_hash: RpcHash,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryRequest {
    pub tx_id: RpcTransactionId,
    pub include_orphan_pool: bool,
    pub filter_transaction_pool: bool,
}

// impl GetMempoolEntryRequest {
//     pub fn new(
//         tx_id: RpcTransactionId,
//         include_orphan_pool: bool,
//         filter_transaction_pool: bool
//     ) -> Self {
//         Self {
//             tx_id,
//             include_orphan_pool,
//             filter_transaction_pool
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryResponse {
    pub mempool_entry: RpcMempoolEntry,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesRequest {
    pub include_orphan_pool: bool,
    pub filter_transaction_pool: bool,
}

// impl GetMempoolEntriesRequest {
//     pub fn new(
//         include_orphan_pool: bool,
//         filter_transaction_pool: bool
//     ) -> Self {
//         Self {
//             include_orphan_pool,
//             filter_transaction_pool
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesResponse {
    pub mempool_entries: Vec<RpcMempoolEntry>,
}

// impl GetMempoolEntriesResponse {
//     pub fn new(mempool_entries : Vec<RpcMempoolEntry>) -> Self {
//         Self {
//             mempool_entries
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoResponse {
    pub peer_info: Vec<RpcPeerInfo>,
}

// impl GetConnectedPeerInfoResponse {
//     pub fn new(peer_info : Vec<RpcPeerInfo>) -> Self {
//         Self { peer_info }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerRequest {
    // FIXME check type
    pub peer_address: RpcPeerAddress,
    pub is_permanent: bool,
}

// impl AddPeerRequest {
//     pub fn new(
//         peer_address: RpcPeerAddress,
//         is_permanent : bool,
//     ) -> Self {
//         Self {
//             peer_address,
//             is_permanent
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerResponse {}

// impl AddPeerResponse {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionRequest {
    pub transaction: RpcTransaction,
    pub allow_orphan: bool,
}

// impl SubmitTransactionRequest {
//     pub fn new(
//         transaction: RpcTransaction,
//         allow_orphan: bool,
//     ) -> Self {
//         Self {
//             transaction,
//             allow_orphan
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionResponse {
    pub transaction_id: RpcTransactionId,
}

// impl SubmitTransactionResponse {
//     pub fn new(transaction_id : RpcTransactionId) -> Self {
//         Self { transaction_id }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkRequest {
    // FIXME type
    pub subnetwork_id: String,
}

// impl GetSubnetworkRequest {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkResponse {
    pub gas_limit: u64,
}

// impl GetSubnetworkResponse {
//     pub fn new(gas_limit : u64) -> Self {
//         Self { gas_limit }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualSelectedParentChainFromBlockRequest {
    // FIXME check type
    pub start_hash: RpcHash,
    pub include_accepted_transaction_ids: bool,
}

// impl GetVirtualSelectedParentChainFromBlockRequest {
//     pub fn new(
//         start_hash : RpcHash,
//         include_accepted_transaction_ids : bool,
//     ) -> Self {
//         Self {
//             start_hash,
//             include_accepted_transaction_ids
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualSelectedParentChainFromBlockResponse {
    pub removed_chain_block_hashes: Vec<RpcHash>,
    pub added_chain_block_hashes: Vec<RpcHash>,
    pub accepted_transaction_ids: Vec<RpcAcceptedTransactionIds>,
}

// impl GetVirtualSelectedParentChainFromBlockResponse {
//     pub fn new(
//         removed_chain_block_hashes : Vec<RpcHash>,
//         added_chain_block_hashes : Vec<RpcHash>,
//         accepted_transaction_ids : Vec<RpcAcceptedTransactionIds>,
//     ) -> Self {
//         Self {
//             removed_chain_block_hashes,
//             added_chain_block_hashes,
//             accepted_transaction_ids
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksRequest {
    pub low_hash: RpcHash,
    pub include_blocks: bool,
    pub include_transactions: bool,
}

// impl GetBlocksRequest {
//     pub fn new(
//         low_hash : RpcHash,
//         include_blocks : bool,
//         include_transactions : bool,
//     ) -> Self {
//         Self {
//             low_hash,
//             include_blocks,
//             include_transactions
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksResponse {
    pub block_hashes: Option<Vec<RpcHash>>,
    pub blocks: Option<Vec<RpcBlock>>,
}

// impl GetBlocksResponse {
//     pub fn new(
//         block_hashes : Option<Vec<RpcHash>>,
//         blocks : Option<Vec<RpcBlock>>,
//     ) -> Self {
//         Self {
//             block_hashes,
//             blocks
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockCountRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockCountResponse {
    pub block_count: u64,
    pub header_count: u64,
}

// impl GetBlockCountResponse {
//     pub fn new(
//         block_count : u64,
//         header_count : u64,
//     ) -> Self {
//         Self {
//             block_count,
//             header_count
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoRequest {}

// impl GetBlockDagInfoRequest {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoResponse {
    pub network_type: RpcNetworkType,
    pub block_count: u64,
    pub header_count: u64,
    pub tip_hashes: Vec<RpcHash>,
    pub difficulty: f64,
    // FIXME check type - i64 in gRPC proto
    pub past_median_time: u64,
    pub virtual_parent_hashes: Vec<RpcHash>,
    pub pruning_point_hash: RpcHash,
    pub virtual_daa_score: u64,
}

// impl GetBlockDagInfoResponse {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictRequest {
    pub finality_block_hash: RpcHash,
}

// impl ResolveFinalityConflictRequest {
//     pub fn new(
//         finality_block_hash : RpcHash,
//     ) -> Self {
//         Self {
//             finality_block_hash,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictResponse {}

// impl ResolveFinalityConflictResponse {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersRequest {
    pub start_hash: RpcHash,
    pub limit: u64,
    pub is_ascending: bool,
}

// impl GetHeadersRequest {
//     pub fn new(
//         start_hash : RpcHash,
//         limit : u64,
//         is_ascending : bool,
//     ) -> Self {
//         Self {
//             start_hash,
//             limit,
//             is_ascending,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersResponse {
    pub headers: Vec<RpcHeader>,
}

// impl GetHeadersResponse {
//     pub fn new(
//         headers : Vec<RpcHeader>,
//     ) -> Self {
//         Self {
//             headers
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressRequest {
    pub address: RpcAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressResponse {
    pub balance: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesResponse {
    pub balances: Vec<(RpcAddress, u64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualSelectedParentBlueScoreRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualSelectedParentBlueScoreResponse {
    pub blue_score: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
}

// impl GetUtxosByAddressesResponse {
//     pub fn new(
//         entries : Vec<RpcUtxosByAddressesEntry>,
//     ) -> Self {
//         Self {
//             entries
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct BanRequest {
    // FIXME check type
    pub address: RpcPeerAddress,
}

// impl BanRequest {
//     pub fn new(
//         address: RpcPeerAddress,
//     ) -> Self {
//         Self {
//             address,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct BanResponse {}

// impl BanResponse {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnbanRequest {
    pub address: RpcPeerAddress,
}

// impl UnbanRequest {
//     pub fn new(
//         address: RpcPeerAddress,
//     ) -> Self {
//         Self {
//             address
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnbanResponse {}

// impl UnbanResponse {
//     pub fn new() -> Self {
//         Self { }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondRequest {
    pub window_size: u32,
    pub start_hash: RpcHash,
}

// impl EstimateNetworkHashesPerSecondRequest {
//     pub fn new(
//         window_size : u32,
//         start_hash : RpcHash,
//     ) -> Self {
//         Self {
//             window_size,
//             start_hash,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondResponse {
    pub network_hashes_per_second: u64,
}

// impl EstimateNetworkHashesPerSecondResponse {
//     pub fn new(
//         network_hashes_per_second : u64,
//     ) -> Self {
//         Self {
//             network_hashes_per_second,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
    pub include_orphan_pool: bool,
    pub filter_transaction_pool: bool,
}

// impl GetMempoolEntriesByAddressesRequest {
//     pub fn new(
//         addresses: Vec<RpcAddress>,
//         include_orphan_pool: bool,
//         filter_transaction_pool: bool,
//     ) -> Self {
//         Self {
//             addresses,
//             include_orphan_pool,
//             filter_transaction_pool,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesResponse {
    pub entries: Vec<RpcMempoolEntryByAddress>,
}

// impl GetMempoolEntriesByAddressesResponse {
//     pub fn new(
//         entries: Vec<RpcMempoolEntryByAddress>,
//     ) -> Self {
//         Self {
//             entries
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyResponse {
    pub max_sompi: u64,
    pub circulating_sompi: u64,
}

// impl GetCoinSupplyResponse {
//     pub fn new(
//         max_sompi : u64,
//         circulating_sompi : u64,
//     ) -> Self {
//         Self {
//             max_sompi,
//             circulating_sompi,
//         }
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualSelectedParentChainChangedRequest {
    pub include_accepted_transaction_ids: bool,
    pub command: SubscribeCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualSelectedParentChainChangedResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictsRequest {
    pub command: SubscribeCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictsResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedRequest {
    pub command: SubscribeCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualSelectedParentBlueScoreChangedRequest {
    pub command: SubscribeCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualSelectedParentBlueScoreChangedResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetProcessMetricsRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetProcessMetricsResponse {
    pub uptime: u64,
    pub memory_used: Vec<u64>,
    pub storage_used: Vec<u64>,
    pub grpc_connections: Vec<u32>,
    pub wrpc_connections: Vec<u32>,
    // TBD:
    //  - approx bandwidth consumption
    //  - other connection metrics
    //  - cpu usage
}
