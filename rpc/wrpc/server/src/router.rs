use kaspa_rpc_macros::build_wrpc_server_interface;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;
use std::sync::Arc;
use workflow_rpc::server::prelude::*;

pub trait RpcApiContainer: Send + Sync + 'static {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi>;
    fn verbose(&self) -> bool {
        false
    }
}

pub enum RouterTarget {
    Server,
    Connection,
}

pub struct Router<ServerContext, ConnectionContext>
where
    ServerContext: RpcApiContainer + Clone,
    ConnectionContext: RpcApiContainer + Clone,
{
    pub interface: Arc<Interface<ServerContext, ConnectionContext, RpcApiOps>>,
}

impl<ServerContext, ConnectionContext> Router<ServerContext, ConnectionContext>
where
    ServerContext: RpcApiContainer + Clone,
    ConnectionContext: RpcApiContainer + Clone,
{
    pub fn new(server_context: ServerContext) -> Self {
        #[allow(unreachable_patterns)]
        let interface = build_wrpc_server_interface!(
            server_context,
            RouterTarget::Server,
            ServerContext,
            ConnectionContext,
            RpcApiOps,
            [
                GetInfo,
                Ping,
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
            ]
        );

        Router { interface }
    }
}
