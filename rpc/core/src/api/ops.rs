use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use workflow_core::{enums::Describe};

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
        GetVirtualSelectedParentChainFromBlock,
        GetBlocks,
        GetBlockCount,
        GetBlockDagInfo,
        ResolveFinalityConflict,
        Shutdown,
        GetHeaders,
        GetUtxosByAddresses,
        GetBalanceByAddress,
        GetBalancesByAddresses,
        GetVirtualSelectedParentBlueScore,
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
        NotifyVirtualSelectedParentChainChanged,
        NotifyVirtualSelectedParentBlueScoreChanged,

        // ~
        Subscribe,
        Unsubscribe,

        // Server to client notification
        Notification,
    }
//});

impl From<RpcApiOps> for u32 {
    fn from(item: RpcApiOps) -> Self {
        item as u32
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum SubscribeCommand {
    Start = 0,
    Stop = 1,
}

impl From<SubscribeCommand> for i32 {
    fn from(item: SubscribeCommand) -> Self {
        item as i32
    }
}

impl From<i32> for SubscribeCommand {
    // We make this conversion infallible by falling back to Start from any unexpected value.
    fn from(item: i32) -> Self {
        if item == 1 {
            SubscribeCommand::Stop
        } else {
            SubscribeCommand::Start
        }
    }
}
