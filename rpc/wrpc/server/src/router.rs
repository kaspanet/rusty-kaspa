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

/// Accessor to the [`RpcApi`] that may reside within
/// different structs.
pub trait RpcApiContainer: Send + Sync + 'static {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi>;
    fn verbose(&self) -> bool {
        false
    }
}

/// [`RouterTarget`] is used during the method and notification
/// registration process to indicate whether the `dyn RpcApi`
/// resides in the `ServerContext` or `ConnectionContext`.
/// When using with rusty-kaspa Server, the RpcApi is local and
/// thus resides in the `ServerContext`, when using with GRPC
/// Proxy, the RpcApi is represented by each forwarding connection
/// and as such resides in the `ConnectionContext`
pub enum RouterTarget {
    Server,
    Connection,
}

/// A wrapper that creates an [`Interface`] instance and initializes
/// RPC methods and notifications agains this interface. The inteface
/// is later given to the RpcServer.  This wrapper exists to allow
/// a single initalization location for both the Kaspad Server and 
/// the GRPC Proxy.
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
    pub fn new(server_context: ServerContext, router_target: RouterTarget) -> Self {

        // The following macro iterates the supplied enum variants taking the variant
        // name and creating an RPC handler using that name. For example, receiving
        // `GetInfo` the macro will conver it to snake name for the function name
        // as well as create `Request` and `Response` typenames and using these typenames
        // it will create the RPC method handler.
        // ... `GetInfo` yields: get_info_call() + GetInfoRequest + GetInfoResponse
        #[allow(unreachable_patterns)]
        let interface = build_wrpc_server_interface!(
            server_context,
            router_target,
            ServerContext,
            ConnectionContext,
            RpcApiOps,
            [
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
            ]
        );

        Router { interface }
    }
}
