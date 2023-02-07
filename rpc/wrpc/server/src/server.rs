use crate::connection::Connection;
use crate::notifications::{ListenerId, NotificationManager};
use crate::result::Result;
use crate::service::Options;
use rpc_core::api::rpc::RpcApi;
use rpc_core::NotificationMessage;
use rpc_grpc::client::RpcApiGrpc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use workflow_core::channel::Sender;
use workflow_log::*;
use workflow_rpc::server::prelude::*;
use workflow_rpc::server::result::Result as WrpcResult;
use workflow_rpc::types::*;

pub struct ConnectionManagerInner {
    pub id: AtomicU64,
    pub encoding: Encoding,
    pub sockets: Mutex<HashMap<u64, Connection>>,
    pub rpc_api: Option<Arc<dyn RpcApi>>,
    pub notifications: NotificationManager,
    pub options: Arc<Options>,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ConnectionManagerInner>,
}

impl Server {
    pub fn new(tasks: usize, encoding: Encoding, rpc_api: Option<Arc<dyn RpcApi>>, options: Arc<Options>) -> Self {
        Server {
            inner: Arc::new(ConnectionManagerInner {
                id: AtomicU64::new(0),
                encoding,
                sockets: Mutex::new(HashMap::new()),
                rpc_api,
                options,
                notifications: NotificationManager::new(tasks),
            }),
        }
    }

    pub async fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        log_info!("WebSocket connected: {}", peer);
        let id = self.inner.id.fetch_add(1, Ordering::SeqCst);

        if let Some(grpc_proxy_address) = &self.inner.options.grpc_proxy_address {
            log_info!("Routing wrpc://{peer} -> {grpc_proxy_address}");
            let grpc =
                RpcApiGrpc::connect(true, grpc_proxy_address.to_owned()).await.map_err(|e| WebSocketError::Other(e.to_string()))?;
            // log_trace!("starting gRPC");
            grpc.start().await;
            // log_trace!("gRPC started...");
            let grpc = Arc::new(grpc);
            // log_trace!("Creating proxy relay...");
            Ok(Connection::new(id, peer, messenger, Some(grpc)))
        } else {
            let connection = Connection::new(id, peer, messenger, None);
            self.inner.sockets.lock()?.insert(id, connection.clone());
            Ok(connection)
        }
    }

    pub async fn disconnect(&self, connection: Connection) {
        log_info!("WebSocket disconnected: {}", connection.peer());

        self.inner.sockets.lock().unwrap().remove(&connection.id());

        let rpc_api = self.get_rpc_api(&connection);
        self.inner.notifications.disconnect(rpc_api, connection).await;
    }

    pub fn get_rpc_api(&self, connection: &Connection) -> Arc<dyn RpcApi> {
        if self.inner.options.grpc_proxy_address.is_some() {
            connection.get_rpc_api()
        } else {
            self.inner.rpc_api.as_ref().expect("invalid access: Server is missing RpcApi while inner.proxy is present").clone()
        }
    }

    pub fn verbose(&self) -> bool {
        self.inner.options.verbose
    }

    pub fn notification_ingest(&self) -> Sender<Arc<NotificationMessage>> {
        self.inner.notifications.ingest.sender.clone()
    }

    pub fn register_notification_listener(&self, id: ListenerId, connection: Connection) {
        self.inner.notifications.register_notification_listener(id, connection)
    }

    /// Creates a WebSocket [`Message`] that can be posted to the connection ([`Messenger`]) sink
    /// directly.
    pub fn create_serialiaed_notification_message<Ops, Msg>(&self, op: Ops, msg: Msg) -> WrpcResult<Message>
    where
        Ops: OpsT,
        Msg: MsgT,
    {
        match self.inner.encoding {
            Encoding::Borsh => workflow_rpc::server::protocol::borsh::create_serialized_notification_message(op, msg),
            Encoding::SerdeJson => workflow_rpc::server::protocol::borsh::create_serialized_notification_message(op, msg),
        }
    }
}

pub type ConnectionManagerReference = Arc<Server>;
