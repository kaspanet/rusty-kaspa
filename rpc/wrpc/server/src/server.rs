use crate::{
    collector::{WrpcServiceCollector, WrpcServiceConverter},
    connection::Connection,
    result::Result,
    service::Options,
};
use kaspa_notify::{
    events::EVENT_TYPE_ARRAY,
    listener::ListenerId,
    notifier::Notifier,
    subscriber::{DynSubscriptionManager, Subscriber},
};
use kaspa_rpc_core::{api::rpc::RpcApi, notify::connection::ChannelConnection, Notification};
use kaspa_utils::channel::Channel;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use workflow_log::*;
use workflow_rpc::server::prelude::*;

pub type DynRpcService = Arc<dyn RpcApi>;
pub type NotificationChannel = Channel<Notification>;

pub struct ServerInner {
    pub next_connection_id: AtomicU64,
    pub encoding: Encoding,
    pub sockets: Mutex<HashMap<u64, Connection>>,
    pub rpc_service: DynRpcService,
    pub rpc_channel: NotificationChannel,
    pub rpc_listener_id: ListenerId,
    pub notifier: Arc<Notifier<Notification, Connection>>,
    pub options: Arc<Options>,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ServerInner>,
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
        let converter = Arc::new(WrpcServiceConverter::new());
        let collector = Arc::new(WrpcServiceCollector::new(rpc_channel.receiver(), converter));
        let subscriber = Arc::new(Subscriber::new(rpc_events, subscription_manager, rpc_listener_id));
        let notifier: Arc<Notifier<Notification, Connection>> =
            Arc::new(Notifier::new(rpc_events, vec![collector], vec![subscriber], tasks, WRPC_SERVER));

        Server {
            inner: Arc::new(ServerInner {
                next_connection_id: AtomicU64::new(0),
                encoding,
                sockets: Mutex::new(HashMap::new()),
                rpc_service,
                rpc_channel,
                rpc_listener_id,
                notifier,
                options,
            }),
        }
    }

    pub fn start(&self) {
        // Start the internal notifier
        self.notifier().start();
    }

    pub fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        log_info!("WebSocket connected: {}", peer);
        let id = self.inner.next_connection_id.fetch_add(1, Ordering::SeqCst);
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

        // TODO: determine if messenger should be closed explicitly
        // connection.close();
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

    pub async fn stop(&self) -> Result<()> {
        // Unsubscribe from all notification types
        let listener_id = self.inner.rpc_listener_id;
        for event in EVENT_TYPE_ARRAY.into_iter() {
            self.inner.rpc_service.stop_notify(listener_id, event.into()).await?;
        }

        // Unregister the listener into RPC service & close the channel
        self.inner.rpc_service.unregister_listener(self.inner.rpc_listener_id).await?;
        self.inner.rpc_channel.close();

        // Stop the internal notifier
        self.notifier().stop().await?;

        Ok(())
    }
}
