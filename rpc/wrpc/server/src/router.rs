use crate::connection::*;
use crate::server::*;
use kaspa_rpc_macros::build_wrpc_server_interface;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
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
// #[derive(Clone)]
// pub enum RouterTarget {
//     Server,
//     Connection,
// }

/// A wrapper that creates an [`Interface`] instance and initializes
/// RPC methods and notifications agains this interface. The inteface
/// is later given to the RpcServer.  This wrapper exists to allow
/// a single initalization location for both the Kaspad Server and
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
        // `GetInfo` the macro will conver it to snake name for the function name
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

        interface.method(
            RpcApiOps::Subscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, notification_type: NotificationType| {
                Box::pin(async move {

                    let rpc_api = manager.get_rpc_api(&connection);

                    let id = if let Some(listener_id) = connection.listener_id() {
                        listener_id
                    } else {
                        let id = rpc_api.register_new_listener(manager.notification_ingest());
                        connection.register_notification_listener(id); //, connection.clone());
                        manager.register_notification_listener(id, connection.clone());
                        id
                    };

                    workflow_log::log_trace!("notification subscribe[0x{id:x}] {notification_type:?}");

                    rpc_api.start_notify(id, notification_type).await.map_err(|err| err.to_string())?;

                    Ok(SubscribeResponse::new(id))
                })
            }),
        );

        interface.method(
            RpcApiOps::Unsubscribe,
            workflow_rpc::server::Method::new(move |manager: Server, connection: Connection, notification_type: NotificationType| {
                Box::pin(async move {

                    if let Some(listener_id) = connection.listener_id() {
                        workflow_log::log_trace!("notification unsubscribe[0x{listener_id:x}] {notification_type:?}");
                    } else {
                        workflow_log::log_trace!("notification unsubscribe[N/A] {notification_type:?}");
                    }

                    let rpc_api = manager.get_rpc_api(&connection);

                    if let Some(listener_id) = connection.listener_id() {
                        rpc_api.stop_notify(listener_id, notification_type).await.unwrap_or_else(|err| {
                            format!("wRPC -> RpcApiOps::Unsubscribe error calling stop_notify(): {err}");
                        });
                    }

                    Ok(UnsubscribeResponse {})
                })
            }),
        );

        Router { interface: Arc::new(interface), server_context }
    }
}
