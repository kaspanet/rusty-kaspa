use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_notify::events::EventType;
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

/// API version. Change in this value should result
/// in the client refusing to connect.
pub const RPC_API_VERSION: u16 = 1;
/// API revision. Change in this value denotes
/// backwards-compatible changes.
pub const RPC_API_REVISION: u16 = 0;

#[derive(Describe, Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum RpcApiOps {
    NoOp = 0,

    // connection control (provisional)
    Connect = 1,
    Disconnect = 2,

    // subscription management
    Subscribe = 3,
    Unsubscribe = 4,

    // ~~~

    // Subscription commands for starting/stopping notifications
    NotifyBlockAdded = 10,
    NotifyNewBlockTemplate = 11,
    NotifyUtxosChanged = 12,
    NotifyPruningPointUtxoSetOverride = 13,
    NotifyFinalityConflict = 14,
    NotifyFinalityConflictResolved = 15, // for uniformity purpose only since subscribing to NotifyFinalityConflict means receiving both FinalityConflict and FinalityConflictResolved
    NotifyVirtualDaaScoreChanged = 16,
    NotifyVirtualChainChanged = 17,
    NotifySinkBlueScoreChanged = 18,

    // Notification ops required by wRPC

    // TODO: Remove these ops and use EventType as NotificationOps when workflow_rpc::server::interface::Interface
    //       will be generic over a MethodOps and NotificationOps instead of a single Ops param.
    BlockAddedNotification = 60,
    VirtualChainChangedNotification = 61,
    FinalityConflictNotification = 62,
    FinalityConflictResolvedNotification = 63,
    UtxosChangedNotification = 64,
    SinkBlueScoreChangedNotification = 65,
    VirtualDaaScoreChangedNotification = 66,
    PruningPointUtxoSetOverrideNotification = 67,
    NewBlockTemplateNotification = 68,

    // RPC methods
    /// Ping the node to check if connection is alive
    Ping = 110,
    /// Get metrics for consensus information and node performance
    GetMetrics = 111,
    /// Get system information (RAM available, number of cores, available file descriptors)
    GetSystemInfo = 112,
    /// Get current number of active TCP connections
    GetConnections = 113,
    /// Get state information on the node
    GetServerInfo = 114,
    /// Get the current sync status of the node
    GetSyncStatus = 115,
    /// Returns the network this Kaspad is connected to (Mainnet, Testnet)
    GetCurrentNetwork = 116,
    /// Extracts a block out of the request message and attempts to add it to the DAG Returns an empty response or an error message
    SubmitBlock = 117,
    /// Returns a "template" by which a miner can mine a new block
    GetBlockTemplate = 118,
    /// Returns a list of all the addresses (IP, port) this Kaspad knows and a list of all addresses that are currently banned by this Kaspad
    GetPeerAddresses = 119,
    /// Returns the hash of the current selected tip block of the DAG
    GetSink = 120,
    /// Get information about an entry in the node's mempool
    GetMempoolEntry = 121,
    /// Get a snapshot of the node's mempool
    GetMempoolEntries = 122,
    /// Returns a list of the peers currently connected to this Kaspad, along with some statistics on them
    GetConnectedPeerInfo = 123,
    /// Instructs Kaspad to connect to a given IP address.
    AddPeer = 124,
    /// Extracts a transaction out of the request message and attempts to add it to the mempool Returns an empty response or an error message
    SubmitTransaction = 125,
    /// Requests info on a block corresponding to a given block hash Returns block info if the block is known.
    GetBlock = 126,
    //
    GetSubnetwork = 127,
    //
    GetVirtualChainFromBlock = 128,
    //
    GetBlocks = 129,
    /// Returns the amount of blocks in the DAG
    GetBlockCount = 130,
    /// Returns info on the current state of the DAG
    GetBlockDagInfo = 131,
    //
    ResolveFinalityConflict = 132,
    /// Instructs this node to shut down Returns an empty response or an error message
    Shutdown = 133,
    //
    GetHeaders = 134,
    /// Get a list of available UTXOs for a given address
    GetUtxosByAddresses = 135,
    /// Get a balance for a given address
    GetBalanceByAddress = 136,
    /// Get a balance for a number of addresses
    GetBalancesByAddresses = 137,
    // ?
    GetSinkBlueScore = 138,
    /// Ban a specific peer by it's IP address
    Ban = 139,
    /// Unban a specific peer by it's IP address
    Unban = 140,
    /// Get generic node information
    GetInfo = 141,
    //
    EstimateNetworkHashesPerSecond = 142,
    /// Get a list of mempool entries that belong to a specific address
    GetMempoolEntriesByAddresses = 143,
    /// Get current issuance supply
    GetCoinSupply = 144,
    /// Get DAA Score timestamp estimate
    GetDaaScoreTimestampEstimate = 145,
    /// Extracts a transaction out of the request message and attempts to replace a matching transaction in the mempool with it, applying a mandatory Replace by Fee policy
    SubmitTransactionReplacement = 146,
    /// Fee estimation
    GetFeeEstimate = 147,
    /// Fee estimation (experimental)
    GetFeeEstimateExperimental = 148,
    /// Block color determination by iterating DAG.
    GetCurrentBlockColor = 149,
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
