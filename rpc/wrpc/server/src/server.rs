use crate::{collector::WrpcServiceCollector, connection::Connection, result::Result, service::Options};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{events::EVENT_TYPE_ARRAY, listener::ListenerId, notifier::Notifier, subscriber::Subscriber};
use kaspa_rpc_core::{
    api::rpc::DynRpcService,
    notify::{channel::NotificationChannel, connection::ChannelConnection, mode::NotificationMode},
    Notification,
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
    pub notification_channel: NotificationChannel,
    pub listener_id: ListenerId,
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
        // Either get a core service or be called from the proxy and rely each connection having its own gRPC client
        assert_eq!(
            core_service.is_none(),
            options.grpc_proxy_address.is_some(),
            "invalid setup: Server must exclusively get either a core service or a gRPC server address"
        );

        let rpc_core = if let Some(service) = core_service {
            // Prepare rpc service objects
            let notification_channel = NotificationChannel::default();
            let listener_id = service.notifier().register_new_listener(ChannelConnection::new(notification_channel.sender()));

            // Prepare notification internals
            let enabled_events = EVENT_TYPE_ARRAY[..].into();
            let collector = Arc::new(WrpcServiceCollector::new(notification_channel.receiver()));
            let subscriber = Arc::new(Subscriber::new(enabled_events, service.notifier(), listener_id));
            let wrpc_notifier = Arc::new(Notifier::new(enabled_events, vec![collector], vec![subscriber], tasks, WRPC_SERVER));
            Some(RpcCore { service, notification_channel, listener_id, wrpc_notifier })
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
        log_info!("WebSocket connected: {}", peer);
        let id = self.inner.next_connection_id.fetch_add(1, Ordering::SeqCst);

        let grpc_client = if let Some(grpc_proxy_address) = &self.inner.options.grpc_proxy_address {
            // Provider::GrpcClient

            log_info!("Routing wrpc://{peer} -> {grpc_proxy_address}");
            let grpc_client = GrpcClient::connect(NotificationMode::Direct, grpc_proxy_address.to_owned(), false, None, true)
                .await
                .map_err(|e| WebSocketError::Other(e.to_string()))?;
            // log_trace!("Creating proxy relay...");
            Some(Arc::new(grpc_client))
        } else {
            // Provider::RpcCore

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
        log_info!("WebSocket disconnected: {}", connection.peer());
        if let Some(rpc_core) = &self.inner.rpc_core {
            if let Some(listener_id) = connection.listener_id() {
                rpc_core.wrpc_notifier.unregister_listener(listener_id).unwrap_or_else(|err| {
                    format!("WebSocket {} (disconnected) error unregistering the notification listener: {err}", connection.peer());
                });
            }
        } else {
            let _ = connection.grpc_client().stop().await;
            let _ = connection.grpc_client().disconnect().await;
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

    pub fn verbose(&self) -> bool {
        self.inner.options.verbose
    }

    pub async fn stop(&self) -> Result<()> {
        if let Some(rpc_core) = &self.inner.rpc_core {
            // Unregister the listener into RPC service & close the channel
            rpc_core.wrpc_notifier.unregister_listener(rpc_core.listener_id)?;
            rpc_core.notification_channel.close();

            // Stop the internal notifier
            rpc_core.wrpc_notifier.stop().await?;
        } else {
            // FIXME: check if all existing connections are actually getting a call to self.disconnect(connection)
            //        else do it here
        }
        Ok(())
    }
}
