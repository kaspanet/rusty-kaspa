use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use workflow_core::{enums::Describe, seal};

seal!(0xf916, {
    // ^^^^^ NOTE: This enum is used for binary RPC data exchange, if you 
    //             add any new variants to this enum, please inform the
    //             core development team to facilitate a protocol update.
    //             If making any changes to this code block, please update
    //             to the new seal value reported by the compiler.
    //                    
    #[derive(Describe, Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
    pub enum RpcApiOps {
        AddPeer,
        Ban,
        EstimateNetworkHashesPerSecond,
        GetBalanceByAddress,
        GetBalancesByAddresses,
        GetBlock,
        GetBlockCount,
        GetBlockDagInfo,
        GetBlocks,
        GetBlockTemplate,
        GetCoinSupply,
        GetConnectedPeerInfo,
        GetCurrentNetwork,
        GetHeaders,
        GetInfo,
        GetMempoolEntries,
        GetMempoolEntriesByAddresses,
        GetMempoolEntry,
        GetPeerAddresses,
        GetProcessMetrics,
        GetSelectedTipHash,
        GetSubnetwork,
        GetUtxosByAddresses,
        GetVirtualSelectedParentBlueScore,
        GetVirtualSelectedParentChainFromBlock,
        Ping,
        ResolveFinalityConflict,
        Shutdown,
        SubmitBlock,
        SubmitTransaction,
        Unban,

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

        Notification,
    }
});

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
