use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_notify::events::EventType;
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

//seal!(0x0a6c, {
// ^^^^^ NOTE: This enum is used for binary RPC data exchange, if you
//             add any new variants to this enum, please inform the
//             core development team to facilitate a protocol update.
//             If making any changes to this code block, please update
//             to the new seal value reported by the compiler.
//
//             Also note that this macro produces a const variable
//             named `SEAL`, that can be used during RPC protocol
//             handshake negotiation.
//
#[derive(Describe, Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RpcApiOps {
    Ping = 0,
    GetProcessMetrics,
    GetCurrentNetwork,
    SubmitBlock,
    GetBlockTemplate,
    GetPeerAddresses,
    GetSelectedTipHash,
    GetMempoolEntry,
    GetMempoolEntries,
    GetConnectedPeerInfo,
    AddPeer,
    SubmitTransaction,
    GetBlock,
    GetSubnetwork,
    GetVirtualChainFromBlock,
    GetBlocks,
    GetBlockCount,
    GetBlockDagInfo,
    ResolveFinalityConflict,
    Shutdown,
    GetHeaders,
    GetUtxosByAddresses,
    GetBalanceByAddress,
    GetBalancesByAddresses,
    GetSinkBlueScore,
    Ban,
    Unban,
    GetInfo,
    EstimateNetworkHashesPerSecond,
    GetMempoolEntriesByAddresses,
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

    // Server to client notification
    Notification,

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
//});

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
            _ => unimplemented!(),
        }
    }
}
