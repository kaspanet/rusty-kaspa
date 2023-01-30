use crate::connection::Connection;
use crate::notifications::{ListenerId, NotificationManager};
use crate::result::Result;
// use crate::router::RouterTarget;
use consensus_core::networktype::NetworkType;
use rpc_core::api::rpc::RpcApi;
use rpc_core::NotificationMessage;
use rpc_grpc::client::RpcApiGrpc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use workflow_core::channel::*;
use workflow_rpc::server::prelude::*;

pub struct ConnectionManagerInner {
    pub id: AtomicU64,
    pub sockets: Mutex<HashMap<u64, Connection>>,
    pub rpc_api: Option<Arc<dyn RpcApi>>,
    pub verbose: bool,
    pub proxy_network_type: Option<NetworkType>,
    pub notifications: NotificationManager,
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<ConnectionManagerInner>,
}

impl Server {
    pub fn new(tasks: usize, rpc_api: Option<Arc<dyn RpcApi>>, proxy_network_type: Option<NetworkType>) -> Self {
        Server {
            inner: Arc::new(ConnectionManagerInner {
                id: AtomicU64::new(0),
                sockets: Mutex::new(HashMap::new()),
                rpc_api,
                verbose: true,
                proxy_network_type,
                notifications: NotificationManager::new(tasks),
            }),
        }
    }

    pub async fn connect(&self, peer: &SocketAddr, messenger: Arc<Messenger>) -> Result<Connection> {
        let id = self.inner.id.fetch_add(1, Ordering::SeqCst);

        if let Some(network_type) = &self.inner.proxy_network_type {
            let port = network_type.port();
            let grpc_address = format!("grpc://127.0.0.1:{port}");
            println!("starting grpc client on {grpc_address}");
            let grpc = RpcApiGrpc::connect(grpc_address).await.map_err(|e| WebSocketError::Other(e.to_string()))?;
            grpc.start().await;
            let grpc = Arc::new(grpc);
            Ok(Connection::new(id, peer, messenger, Some(grpc)))
        } else {
            let connection = Connection::new(id, peer, messenger, None);
            self.inner.sockets.lock()?.insert(id, connection.clone());
            Ok(connection)
        }
    }

    pub async fn disconnect(&self, connection: Connection) {
        self.inner.sockets.lock().unwrap().remove(&connection.id());

        let rpc_api = self.get_rpc_api(&connection);
        self.inner.notifications.disconnect(rpc_api, connection).await;
    }

    pub fn get_rpc_api(&self, connection: &Connection) -> Arc<dyn RpcApi> {
        if self.inner.proxy_network_type.is_some() {
            self.inner
                .rpc_api
                .as_ref()
                .expect("invalid access: ConnectionManager is missing RpcApi while inner.proxy is present")
                .clone()
        } else {
            connection.get_rpc_api()
        }
    }

    pub fn verbose(&self) -> bool {
        self.inner.verbose
    }

    pub fn notification_ingest(&self) -> Sender<Arc<NotificationMessage>> {
        self.inner.notifications.ingest.sender.clone()
    }

    pub fn register_notification_listener(&self, id: ListenerId, connection: Connection) {
        self.inner.notifications.register_notification_listener(id, connection)
    }
}

pub type ConnectionManagerReference = Arc<Server>;
