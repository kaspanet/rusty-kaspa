use crate::{connection::*, server::*};
use kaspa_notify::scope::Scope;
use kaspa_rpc_core::{api::ops::RpcApiOps, prelude::*};
use kaspa_rpc_macros::build_wrpc_server_interface;
use std::sync::Arc;
use workflow_rpc::server::prelude::*;

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
                GetSinkBlueScore,
                GetVirtualChainFromBlock,
                Ping,
                ResolveFinalityConflict,
                Shutdown,
                SubmitBlock,
                SubmitTransaction,
                Unban,
            ]
        );

        interface.method(
            RpcApiOps::Subscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, scope: Scope| {
                Box::pin(async move {
                    let rpc_service = manager.rpc_service(&connection);
                    let listener_id = if let Some(listener_id) = connection.listener_id() {
                        listener_id
                    } else {
                        // The only possible case here is a server connected to rpc core.
                        // If the proxy is used, the connection has a gRPC client and the listener id
                        // is always set to Some(ListenerId::default()) by the connection ctor.
                        let notifier = manager
                            .notifier()
                            .unwrap_or_else(|| panic!("Incorrect use: `server::Server` does not carry an internal notifier"));
                        let listener_id = notifier.register_new_listener(connection.clone());
                        connection.register_notification_listener(listener_id);
                        listener_id
                    };
                    workflow_log::log_trace!("notification subscribe[0x{listener_id:x}] {scope:?}");
                    rpc_service.start_notify(listener_id, scope).await.map_err(|err| err.to_string())?;
                    Ok(SubscribeResponse::new(listener_id))
                })
            }),
        );

        interface.method(
            RpcApiOps::Unsubscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, scope: Scope| {
                Box::pin(async move {
                    if let Some(listener_id) = connection.listener_id() {
                        workflow_log::log_trace!("notification unsubscribe[0x{listener_id:x}] {scope:?}");
                        let rpc_service = manager.rpc_service(&connection);
                        rpc_service.stop_notify(listener_id, scope).await.unwrap_or_else(|err| {
                            format!("wRPC -> RpcApiOps::Unsubscribe error calling stop_notify(): {err}");
                        });
                    } else {
                        workflow_log::log_trace!("notification unsubscribe[N/A] {scope:?}");
                    }
                    Ok(UnsubscribeResponse {})
                })
            }),
        );

        Router { interface: Arc::new(interface), server_context }
    }
}
