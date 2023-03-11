use crate::collector::WrpcServiceCollector;
use crate::connection::Connection;
use crate::result::Result;
use crate::service::Options;
use kaspa_grpc_core::channel::NotificationChannel;
use kaspa_notify::events::EVENT_TYPE_ARRAY;
use kaspa_notify::subscriber::{DynSubscriptionManager, Subscriber};
use kaspa_notify::{listener::ListenerId, notifier::Notifier};
use kaspa_rpc_core::notify::connection::ChannelConnection;
use kaspa_rpc_core::{api::rpc::RpcApi, Notification};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use workflow_log::*;
use workflow_rpc::server::prelude::*;

pub type DynRpcService = Arc<dyn RpcApi<ChannelConnection>>;

pub struct ConnectionManagerInner {
    pub id: AtomicU64,
    pub encoding: Encoding,
    pub sockets: Mutex<HashMap<u64, Connection>>,
    pub rpc_service: DynRpcService,
    pub rpc_listener_id: ListenerId,
    pub notifier: Arc<Notifier<Notification, Connection>>,
    pub options: Arc<Options>,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ConnectionManagerInner>,
}

const WRPC_SERVER: &str = "wrpc-server";

impl Server {
    pub fn new(
        tasks: usize,
        encoding: Encoding,
        rpc_service: DynRpcService,
        subscription_manager: DynSubscriptionManager,
        options: Arc<Options>,
    ) -> Self {
        // Prepare rpc service objects
        let rpc_channel = NotificationChannel::default();
        let rpc_listener_id = rpc_service.register_new_listener(ChannelConnection::new(rpc_channel.sender()));

        // Prepare notification internals
        let rpc_events = EVENT_TYPE_ARRAY[..].into();
        let collector = Arc::new(WrpcServiceCollector::new(rpc_channel.receiver()));
        let subscriber = Arc::new(Subscriber::new(rpc_events, subscription_manager, rpc_listener_id));
        let notifier: Arc<Notifier<Notification, Connection>> =
            Arc::new(Notifier::new(rpc_events, vec![collector], vec![subscriber], tasks, WRPC_SERVER));

        Server {
            inner: Arc::new(ConnectionManagerInner {
                id: AtomicU64::new(0),
                encoding,
                sockets: Mutex::new(HashMap::new()),
                rpc_service,
                rpc_listener_id,
                notifier,
                options,
            }),
        }
    }

    pub fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        log_info!("WebSocket connected: {}", peer);
        let id = self.inner.id.fetch_add(1, Ordering::SeqCst);
        let connection = Connection::new(id, peer, messenger);
        self.inner.sockets.lock()?.insert(id, connection.clone());
        Ok(connection)
    }

    pub fn disconnect(&self, connection: Connection) {
        log_info!("WebSocket disconnected: {}", connection.peer());

        if let Some(listener_id) = connection.listener_id() {
            self.notifier().unregister_listener(listener_id).unwrap_or_else(|err| {
                format!("WebSocket {} (disconnected) error unregistering the notification listener: {err}", connection.peer());
            })
        }
        self.inner.sockets.lock().unwrap().remove(&connection.id());
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, Connection>> {
        self.inner.notifier.clone()
    }

    pub fn rpc_service(&self, _connection: &Connection) -> DynRpcService {
        self.inner.rpc_service.clone()
    }

    pub fn verbose(&self) -> bool {
        self.inner.options.verbose
    }
}

pub type ConnectionManagerReference = Arc<Server>;
