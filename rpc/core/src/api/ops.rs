use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_notify::events::EventType;
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

/// Rpc Api version (4 x short values); First short is reserved.
/// The version format is as follows: `[reserved, major, minor, patch]`.
/// The difference in the major version value indicates breaking binary API changes
/// (i.e. changes in non-versioned model data structures)
/// If such change occurs, BorshRPC-client should refuse to connect to the
/// server and should request a client-side upgrade.  JsonRPC-client may opt-in to
/// continue interop, but data structures should handle mutations by pre-filtering
/// or using Serde attributes. This applies only to RPC infrastructure that uses internal
/// data structures and does not affect gRPC. gRPC should issue and handle its
/// own versioning.
pub const RPC_API_VERSION: [u16; 4] = [0, 1, 0, 0];

#[derive(Describe, Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RpcApiOps {
    /// Ping the node to check if connection is alive
    Ping = 0,
    /// Get metrics for consensus information and node performance
    GetMetrics,
    /// Get state information on the node
    GetServerInfo,
    /// Get the current sync status of the node
    GetSyncStatus,
    /// Returns the network this Kaspad is connected to (Mainnet, Testnet)
    GetCurrentNetwork,
    /// Extracts a block out of the request message and attempts to add it to the DAG Returns an empty response or an error message
    SubmitBlock,
    /// Returns a "template" by which a miner can mine a new block
    GetBlockTemplate,
    /// Returns a list of all the addresses (IP, port) this Kaspad knows and a list of all addresses that are currently banned by this Kaspad
    GetPeerAddresses,
    /// Returns the hash of the current selected tip block of the DAG
    GetSelectedTipHash,
    /// Get information about an entry in the node's mempool
    GetMempoolEntry,
    /// Get a snapshot of the node's mempool
    GetMempoolEntries,
    /// Returns a list of the peers currently connected to this Kaspad, along with some statistics on them
    GetConnectedPeerInfo,
    /// Instructs Kaspad to connect to a given IP address.
    AddPeer,
    /// Extracts a transaction out of the request message and attempts to add it to the mempool Returns an empty response or an error message
    SubmitTransaction,
    /// Requests info on a block corresponding to a given block hash Returns block info if the block is known.
    GetBlock,
    //
    GetSubnetwork,
    //
    GetVirtualChainFromBlock,
    //
    GetBlocks,
    /// Returns the amount of blocks in the DAG
    GetBlockCount,
    /// Returns info on the current state of the DAG
    GetBlockDagInfo,
    //
    ResolveFinalityConflict,
    /// Instructs this node to shut down Returns an empty response or an error message
    Shutdown,
    //
    GetHeaders,
    /// Get a list of available UTXOs for a given address
    GetUtxosByAddresses,
    /// Get a balance for a given address
    GetBalanceByAddress,
    /// Get a balance for a number of addresses
    GetBalancesByAddresses,
    // ?
    GetSinkBlueScore,
    /// Ban a specific peer by it's IP address
    Ban,
    /// Unban a specific peer by it's IP address
    Unban,
    /// Get generic node information
    GetInfo,
    //
    EstimateNetworkHashesPerSecond,
    /// Get a list of mempool entries that belong to a specific address
    GetMempoolEntriesByAddresses,
    /// Get current issuance supply
    GetCoinSupply,

    // Subscription commands for starting/stopping notifications
    NotifyBlockAdded,
    NotifyNewBlockTemplate,
    NotifyUtxosChanged,
    NotifyPruningPointUtxoSetOverride,
    NotifyFinalityConflict,
    NotifyFinalityConflictResolved, // for uniformity purpose only since subscribing to NotifyFinalityConflict means receiving both FinalityConflict and FinalityConflictResolved
    NotifyVirtualDaaScoreChanged,
    NotifyVirtualChainChanged,
    NotifySinkBlueScoreChanged,

    // ~
    Subscribe,
    Unsubscribe,

    // Notification ops required by wRPC
    // TODO: Remove these ops and use EventType as NotificationOps when workflow_rpc::server::interface::Interface
    //       will be generic over a MethodOps and NotificationOps instead of a single Ops param.
    BlockAddedNotification,
    VirtualChainChangedNotification,
    FinalityConflictNotification,
    FinalityConflictResolvedNotification,
    UtxosChangedNotification,
    SinkBlueScoreChangedNotification,
    VirtualDaaScoreChangedNotification,
    PruningPointUtxoSetOverrideNotification,
    NewBlockTemplateNotification,
}

impl RpcApiOps {
    pub fn is_subscription(&self) -> bool {
        matches!(
            self,
            RpcApiOps::NotifyBlockAdded
                | RpcApiOps::NotifyNewBlockTemplate
                | RpcApiOps::NotifyUtxosChanged
                | RpcApiOps::NotifyVirtualChainChanged
                | RpcApiOps::NotifyPruningPointUtxoSetOverride
                | RpcApiOps::NotifyFinalityConflict
                | RpcApiOps::NotifyFinalityConflictResolved
                | RpcApiOps::NotifySinkBlueScoreChanged
                | RpcApiOps::NotifyVirtualDaaScoreChanged
                | RpcApiOps::Subscribe
                | RpcApiOps::Unsubscribe
        )
    }
}

impl From<RpcApiOps> for u32 {
    fn from(item: RpcApiOps) -> Self {
        item as u32
    }
}

// TODO: Remove this conversion when workflow_rpc::server::interface::Interface
//       will be generic over a MethodOps and NotificationOps instead of a single Ops param.
impl From<EventType> for RpcApiOps {
    fn from(item: EventType) -> Self {
        match item {
            EventType::BlockAdded => RpcApiOps::BlockAddedNotification,
            EventType::VirtualChainChanged => RpcApiOps::VirtualChainChangedNotification,
            EventType::FinalityConflict => RpcApiOps::FinalityConflictNotification,
            EventType::FinalityConflictResolved => RpcApiOps::FinalityConflictResolvedNotification,
            EventType::UtxosChanged => RpcApiOps::UtxosChangedNotification,
            EventType::SinkBlueScoreChanged => RpcApiOps::SinkBlueScoreChangedNotification,
            EventType::VirtualDaaScoreChanged => RpcApiOps::VirtualDaaScoreChangedNotification,
            EventType::PruningPointUtxoSetOverride => RpcApiOps::PruningPointUtxoSetOverrideNotification,
            EventType::NewBlockTemplate => RpcApiOps::NewBlockTemplateNotification,
        }
    }
}
