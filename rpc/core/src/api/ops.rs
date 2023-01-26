use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use workflow_core::enums::u32_try_from;

u32_try_from! {
    #[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
    #[repr(u32)]
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
        StopNotifyingUtxosChanged,

        NotifyPruningPointUtxoSetOverride,
        StopNotifyingPruningPointUtxoSetOverride,

        NotifyVirtualDaaScoreChanged,
        NotifyVirtualSelectedParentChainChanged,
        NotifyVirtualSelectedParentBlueScoreChanged,
        NotifyFinalityConflicts,

        // gRPC v1 notification messages
        // TODO @tiram - review handling
        // FinalityConflictNotification,
        // FinalityConflictResolvedNotification,
        // UtxosChangedNotification,
        // VirtualSelectedParentBlueScoreChangedNotification,
        // PruningPointUtxoSetOverrideNotification,
        // VirtualDaaScoreChangedNotification,
        // VirtualSelectedParentChainChangedNotification,

        // Server to client notification
        Notification,
    }
}

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

#[cfg(test)]
mod tests {
    use super::RpcApiOps;

    #[test]
    fn test_rpc_api_ops_convert() {
        assert_eq!(0_u32, u32::from(RpcApiOps::Ping));
    }
}
