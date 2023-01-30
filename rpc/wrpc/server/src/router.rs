use crate::connection::*;
use crate::manager::*;
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
#[derive(Clone)]
pub enum RouterTarget {
    Server,
    Connection,
}

/// A wrapper that creates an [`Interface`] instance and initializes
/// RPC methods and notifications agains this interface. The inteface
/// is later given to the RpcServer.  This wrapper exists to allow
/// a single initalization location for both the Kaspad Server and
/// the GRPC Proxy.
pub struct Router {
    pub interface: Arc<Interface<ConnectionManager, Connection, RpcApiOps>>,
    pub server_context: ConnectionManager,
}

impl Router {
    pub fn new(server_context: ConnectionManager, _router_target: RouterTarget) -> Self {
        let router_target = server_context.router_target();

        // The following macro iterates the supplied enum variants taking the variant
        // name and creating an RPC handler using that name. For example, receiving
        // `GetInfo` the macro will conver it to snake name for the function name
        // as well as create `Request` and `Response` typenames and using these typenames
        // it will create the RPC method handler.
        // ... `GetInfo` yields: get_info_call() + GetInfoRequest + GetInfoResponse
        #[allow(unreachable_patterns)]
        let mut interface = build_wrpc_server_interface!(
            server_context.clone(),
            router_target,
            ConnectionManager,
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

        let router_target_ = router_target.clone();
        interface.method(
            RpcApiOps::Subscribe,
            workflow_rpc::server::Method::new(
                move |manager: ConnectionManager, connection: Connection, notification_type: NotificationType| {
                    let router_target = router_target_.clone();
                    Box::pin(async move {
                        workflow_log::log_trace!("notification request {:?}", notification_type);

                        let api = match &router_target {
                            RouterTarget::Server => manager.get_rpc_api(),
                            RouterTarget::Connection => connection.get_rpc_api(),
                        };

                        let listener_id = if let Some(listener_id) = connection.listener_id() {
                            listener_id
                        } else {
                            let id = api.register_new_listener(manager.notification_ingest());
                            connection.register_notification_listener(id); //, connection.clone());
                            manager.register_notification_listener(id, connection.clone());
                            id
                        };

                        let _result = api.start_notify(listener_id, notification_type).await;

                        Ok(())
                    })
                },
            ),
        );

        let router_target_ = router_target.clone();
        interface.method(
            RpcApiOps::Unsubscribe,
            workflow_rpc::server::Method::new(
                move |manager: ConnectionManager, connection: Connection, notification_type: NotificationType| {
                    let router_target = router_target_.clone();
                    Box::pin(async move {
                        workflow_log::log_trace!("notification request {:?}", notification_type);

                        let api = match router_target {
                            RouterTarget::Server => manager.get_rpc_api(),
                            RouterTarget::Connection => connection.get_rpc_api(),
                        };

                        let listener_id = if let Some(listener_id) = connection.listener_id() {
                            listener_id
                        } else {
                            let id = api.register_new_listener(manager.notification_ingest());
                            connection.register_notification_listener(id); //, connection.clone());
                            manager.register_notification_listener(id, connection.clone());
                            id
                        };

                        let _result = api.start_notify(listener_id, notification_type).await;

                        Ok(())
                    })
                },
            ),
        );

        Router { interface: Arc::new(interface), server_context }
    }
}
