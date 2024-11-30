use crate::{
    collector::{WrpcServiceCollector, WrpcServiceConverter},
    connection::Connection,
    result::Result,
    service::Options,
};
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
    api::rpc::DynRpcService,
    notify::{channel::NotificationChannel, connection::ChannelConnection},
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
    pub rpc_core: RpcCore,
    pub options: Arc<Options>,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ServerInner>,
}

const WRPC_SERVER: &str = "wrpc-server";

impl Server {
    pub fn new(tasks: usize, encoding: Encoding, service: Arc<RpcCoreService>, options: Arc<Options>) -> Self {
        // This notifier UTXOs subscription granularity to rpc-core notifier
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);
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

        Server {
            inner: Arc::new(ServerInner {
                next_connection_id: AtomicU64::new(0),
                _encoding: encoding,
                sockets: Mutex::new(HashMap::new()),
                rpc_core: RpcCore { service, wrpc_notifier },
                options,
            }),
        }
    }

    pub fn start(&self) {
        self.inner.rpc_core.wrpc_notifier.clone().start();
    }

    pub async fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        // log_trace!("WebSocket connected: {}", peer);
        // Generate a new connection ID
        let id = self.inner.next_connection_id.fetch_add(1, Ordering::SeqCst);

        // Create the connection without gRPC client handling
        let connection = Connection::new(id, peer, messenger);

        // Insert the new connection into the sockets map
        self.inner.sockets.lock()?.insert(id, connection.clone());

        Ok(connection)
    }

    pub async fn disconnect(&self, connection: Connection) {
        // log_info!("WebSocket disconnected: {}", connection.peer());
        // Unregister available subscriptions of disconnecting connection
        if let Some(listener_id) = connection.listener_id() {
            self.inner.rpc_core.wrpc_notifier.unregister_listener(listener_id).unwrap_or_else(|err| {
                log_error!("WebSocket {} (disconnected) error unregistering the notification listener: {err}", connection.peer());
            });
        }

        // Remove the connection from the sockets
        self.inner.sockets.lock().unwrap().remove(&connection.id());
        // FIXME: determine if messenger should be closed explicitly
        // connection.close();
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<WrpcNotifier> {
        self.inner.rpc_core.wrpc_notifier.clone()
    }

    pub fn rpc_service(&self) -> DynRpcService {
        self.inner.rpc_core.service.clone()
    }

    pub async fn start_notify(&self, connection: &Connection, scope: Scope) -> RpcResult<()> {
        let listener_id = if let Some(listener_id) = connection.listener_id() {
            listener_id
        } else {
            // The only possible case here is a server connected to rpc core.
            // Register a new listener if one is not already set.
            let notifier = self.notifier();
            let listener_id = notifier.register_new_listener(connection.clone(), ListenerLifespan::Dynamic);
            connection.register_notification_listener(listener_id);
            listener_id
        };

        workflow_log::log_trace!("notification subscribe[0x{listener_id:x}] {scope:?}");
        self.inner.rpc_core.wrpc_notifier.clone().try_start_notify(listener_id, scope)?;

        Ok(())
    }

    pub async fn stop_notify(&self, connection: &Connection, scope: Scope) -> RpcResult<()> {
        if let Some(listener_id) = connection.listener_id() {
            workflow_log::log_trace!("notification unsubscribe[0x{listener_id:x}] {scope:?}");
            self.inner.rpc_core.wrpc_notifier.clone().try_stop_notify(listener_id, scope)?;
        } else {
            workflow_log::log_trace!("notification unsubscribe[N/A] {scope:?}");
        }

        Ok(())
    }

    pub fn verbose(&self) -> bool {
        self.inner.options.verbose
    }

    pub async fn join(&self) -> Result<()> {
        self.inner.rpc_core.wrpc_notifier.join().await?;

        // FIXME: check if all existing connections are actually getting a call to self.disconnect(connection) or do it here
        Ok(())
    }
}
