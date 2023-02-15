use workflow_core::enums::u32_try_from;

u32_try_from! {
    #[derive(Clone, Debug, PartialEq, Eq)]
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
        NotifyPruningPointUtxoSetOverride,
        NotifyFinalityConflict,
        NotifyVirtualDaaScoreChanged,
        NotifyVirtualSelectedParentChainChanged,
        NotifyVirtualSelectedParentBlueScoreChanged,

        // Server to client notification
        Notification,
    }
}

impl From<RpcApiOps> for u32 {
    fn from(item: RpcApiOps) -> Self {
        item as u32
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
