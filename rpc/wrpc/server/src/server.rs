use crate::{
    collector::{WrpcServiceCollector, WrpcServiceConverter},
    connection::Connection,
    result::Result,
    service::Options,
};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{
    connection::ChannelType,
    events::EVENT_TYPE_ARRAY,
    listener::ListenerLifespan,
    notifier::Notifier,
    scope::Scope,
    subscriber::Subscriber,
    subscription::{MutationPolicies, UtxosChangedMutationPolicy},
};
use kaspa_rpc_core::{
    api::rpc::{DynRpcService, RpcApi},
    notify::{channel::NotificationChannel, connection::ChannelConnection, mode::NotificationMode},
    Notification, RpcResult,
};
use kaspa_rpc_service::service::RpcCoreService;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use workflow_log::*;
use workflow_rpc::server::prelude::*;

pub type WrpcNotifier = Notifier<Notification, Connection>;

struct RpcCore {
    pub service: Arc<RpcCoreService>,
    pub wrpc_notifier: Arc<WrpcNotifier>,
}

struct ServerInner {
    pub next_connection_id: AtomicU64,
    pub _encoding: Encoding,
    pub sockets: Mutex<HashMap<u64, Connection>>,
    pub rpc_core: Option<RpcCore>,
    pub options: Arc<Options>,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ServerInner>,
}

const WRPC_SERVER: &str = "wrpc-server";

impl Server {
    pub fn new(tasks: usize, encoding: Encoding, core_service: Option<Arc<RpcCoreService>>, options: Arc<Options>) -> Self {
        // This notifier UTXOs subscription granularity to rpc-core notifier
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);

        // Either get a core service or be called from the proxy and rely each connection having its own gRPC client
        assert_eq!(
            core_service.is_none(),
            options.grpc_proxy_address.is_some(),
            "invalid setup: Server must exclusively get either a core service or a gRPC server address"
        );

        let rpc_core = if let Some(service) = core_service {
            // Prepare rpc service objects
            let notification_channel = NotificationChannel::default();
            let listener_id = service.notifier().register_new_listener(
                ChannelConnection::new(WRPC_SERVER, notification_channel.sender(), ChannelType::Closable),
                ListenerLifespan::Static(policies),
            );

            // Prepare notification internals
            let enabled_events = EVENT_TYPE_ARRAY[..].into();
            let converter = Arc::new(WrpcServiceConverter::new());
            let collector = Arc::new(WrpcServiceCollector::new(WRPC_SERVER, notification_channel.receiver(), converter));
            let subscriber = Arc::new(Subscriber::new(WRPC_SERVER, enabled_events, service.notifier(), listener_id));
            let wrpc_notifier = Arc::new(Notifier::new(
                WRPC_SERVER,
                enabled_events,
                vec![collector],
                vec![subscriber],
                service.subscription_context(),
                tasks,
                policies,
            ));
            Some(RpcCore { service, wrpc_notifier })
        } else {
            None
        };

        Server {
            inner: Arc::new(ServerInner {
                next_connection_id: AtomicU64::new(0),
                _encoding: encoding,
                sockets: Mutex::new(HashMap::new()),
                rpc_core,
                options,
            }),
        }
    }

    pub fn start(&self) {
        if let Some(rpc_core) = &self.inner.rpc_core {
            // Start the internal notifier
            rpc_core.wrpc_notifier.clone().start();
        }
    }

    pub async fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        // log_trace!("WebSocket connected: {}", peer);
        let id = self.inner.next_connection_id.fetch_add(1, Ordering::SeqCst);

        let grpc_client = if let Some(grpc_proxy_address) = &self.inner.options.grpc_proxy_address {
            // Provider::GrpcClient

            log_info!("Routing wrpc://{peer} -> {grpc_proxy_address}");
            let grpc_client = GrpcClient::connect_with_args(
                NotificationMode::Direct,
                grpc_proxy_address.to_owned(),
                None,
                false,
                None,
                true,
                None,
                Default::default(),
            )
            .await
            .map_err(|e| WebSocketError::Other(e.to_string()))?;
            // log_trace!("Creating proxy relay...");
            Some(Arc::new(grpc_client))
        } else {
            None
        };
        let connection = Connection::new(id, peer, messenger, grpc_client);
        if self.inner.options.grpc_proxy_address.is_some() {
            // log_trace!("starting gRPC");
            connection.grpc_client().start(Some(connection.grpc_client_notify_target())).await;
            // log_trace!("gRPC started...");
        }
        self.inner.sockets.lock()?.insert(id, connection.clone());
        Ok(connection)
    }

    pub async fn disconnect(&self, connection: Connection) {
        // log_info!("WebSocket disconnected: {}", connection.peer());
        if let Some(rpc_core) = &self.inner.rpc_core {
            if let Some(listener_id) = connection.listener_id() {
                rpc_core.wrpc_notifier.unregister_listener(listener_id).unwrap_or_else(|err| {
                    log_error!("WebSocket {} (disconnected) error unregistering the notification listener: {err}", connection.peer());
                });
            }
        } else {
            let _ = connection.grpc_client().disconnect().await;
            let _ = connection.grpc_client().join().await;
        }

        self.inner.sockets.lock().unwrap().remove(&connection.id());

        // FIXME: determine if messenger should be closed explicitly
        // connection.close();
    }

    #[inline(always)]
    pub fn notifier(&self) -> Option<Arc<WrpcNotifier>> {
        self.inner.rpc_core.as_ref().map(|x| x.wrpc_notifier.clone())
    }

    pub fn rpc_service(&self, connection: &Connection) -> DynRpcService {
        if let Some(rpc_core) = &self.inner.rpc_core {
            rpc_core.service.clone()
        } else {
            connection.grpc_client()
        }
    }

    pub async fn start_notify(&self, connection: &Connection, scope: Scope) -> RpcResult<()> {
        let listener_id = if let Some(listener_id) = connection.listener_id() {
            listener_id
        } else {
            // The only possible case here is a server connected to rpc core.
            // If the proxy is used, the connection has a gRPC client and the listener id
            // is always set to Some(ListenerId::default()) by the connection ctor.
            let notifier =
                self.notifier().unwrap_or_else(|| panic!("Incorrect use: `server::Server` does not carry an internal notifier"));
            let listener_id = notifier.register_new_listener(connection.clone(), ListenerLifespan::Dynamic);
            connection.register_notification_listener(listener_id);
            listener_id
        };
        workflow_log::log_trace!("notification subscribe[0x{listener_id:x}] {scope:?}");
        if let Some(rpc_core) = &self.inner.rpc_core {
            rpc_core.wrpc_notifier.clone().try_start_notify(listener_id, scope)?;
        } else {
            connection.grpc_client().start_notify(listener_id, scope).await?;
        }
        Ok(())
    }

    pub async fn stop_notify(&self, connection: &Connection, scope: Scope) -> RpcResult<()> {
        if let Some(listener_id) = connection.listener_id() {
            workflow_log::log_trace!("notification unsubscribe[0x{listener_id:x}] {scope:?}");
            if let Some(rpc_core) = &self.inner.rpc_core {
                rpc_core.wrpc_notifier.clone().try_stop_notify(listener_id, scope)?;
            } else {
                connection.grpc_client().stop_notify(listener_id, scope).await?;
            }
        } else {
            workflow_log::log_trace!("notification unsubscribe[N/A] {scope:?}");
        }
        Ok(())
    }

    pub fn verbose(&self) -> bool {
        self.inner.options.verbose
    }

    pub async fn join(&self) -> Result<()> {
        if let Some(rpc_core) = &self.inner.rpc_core {
            // Wait for the internal notifier to stop
            rpc_core.wrpc_notifier.join().await?;
        } else {
            // FIXME: check if all existing connections are actually getting a call to self.disconnect(connection)
            //        else do it here
        }
        Ok(())
    }
}
