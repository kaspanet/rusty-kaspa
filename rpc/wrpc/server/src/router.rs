use crate::{connection::*, server::*};
use kaspa_notify::scope::Scope;
use kaspa_rpc_core::{api::ops::RpcApiOps, prelude::*};
use kaspa_rpc_macros::build_wrpc_server_interface;
use std::sync::Arc;
use workflow_rpc::server::prelude::*;
use workflow_serializer::prelude::*;

/// A wrapper that creates an [`Interface`] instance and initializes
/// RPC methods and notifications against this interface. The interface
/// is later given to the RpcServer.  This wrapper exists to allow
/// a single initialization location for both the Kaspad Server and
/// the GRPC Proxy.
pub struct Router {
    pub interface: Arc<Interface<Server, Connection, RpcApiOps>>,
    pub server_context: Server,
}

impl Router {
    pub fn new(server_context: Server) -> Self {
        // let router_target = server_context.router_target();

        // The following macro iterates the supplied enum variants taking the variant
        // name and creating an RPC handler using that name. For example, receiving
        // `GetInfo` the macro will convert it to snake name for the function name
        // as well as create `Request` and `Response` typenames and using these typenames
        // it will create the RPC method handler.
        // ... `GetInfo` yields: get_info_call() + GetInfoRequest + GetInfoResponse
        #[allow(unreachable_patterns)]
        let mut interface = build_wrpc_server_interface!(
            server_context.clone(),
            Server,
            Connection,
            RpcApiOps,
            [
                Ping,
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
                GetCurrentBlockColor,
                GetCoinSupply,
                GetConnectedPeerInfo,
                GetCurrentNetwork,
                GetDaaScoreTimestampEstimate,
                GetFeeEstimate,
                GetFeeEstimateExperimental,
                GetHeaders,
                GetInfo,
                GetInfo,
                GetMempoolEntries,
                GetMempoolEntriesByAddresses,
                GetMempoolEntry,
                GetMetrics,
                GetConnections,
                GetPeerAddresses,
                GetServerInfo,
                GetSink,
                GetSinkBlueScore,
                GetSubnetwork,
                GetSyncStatus,
                GetSystemInfo,
                GetUtxosByAddresses,
                GetVirtualChainFromBlock,
                ResolveFinalityConflict,
                Shutdown,
                SubmitBlock,
                SubmitTransaction,
                SubmitTransactionReplacement,
                Unban,
            ]
        );

        interface.method(
            RpcApiOps::Subscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, scope: Serializable<Scope>| {
                Box::pin(async move {
                    manager.start_notify(&connection, scope.into_inner()).await.map_err(|err| err.to_string())?;
                    Ok(Serializable(SubscribeResponse::new(connection.id())))
                })
            }),
        );

        interface.method(
            RpcApiOps::Unsubscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, scope: Serializable<Scope>| {
                Box::pin(async move {
                    manager.stop_notify(&connection, scope.into_inner()).await.unwrap_or_else(|err| {
                        workflow_log::log_trace!("wRPC server -> error calling stop_notify(): {err}");
                    });
                    Ok(Serializable(UnsubscribeResponse {}))
                })
            }),
        );

        Router { interface: Arc::new(interface), server_context }
    }
}
