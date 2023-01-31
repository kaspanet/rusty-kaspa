use crate::notifications::*;
use rpc_core::api::rpc::RpcApi;
use rpc_grpc::client::RpcApiGrpc;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use workflow_rpc::server::prelude::*;

#[derive(Debug)]
pub struct ConnectionInner {
    pub id: u64,
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
    pub grpc_api: Option<Arc<RpcApiGrpc>>,
    // not using an atomic in case an Id will change type in the future...
    pub listener_id: Mutex<Option<ListenerId>>,
    pub subscriptions: Mutex<HashSet<ListenerId>>,
}

impl ConnectionInner {}

/// ConnectionContext represents a currently connected WebSocket RPC channel.
/// This struct owns a [`Messenger`] that has [`Messenger::notify`]
/// function that can be used to post notifications to the connection.
/// [`Messenger::close`] function can be used to terminate the connection
/// asynchronously.
#[derive(Debug, Clone)]
pub struct Connection {
    inner: Arc<ConnectionInner>,
}

impl Connection {
    pub fn new(id: u64, peer: &SocketAddr, messenger: Arc<Messenger>, grpc_api: Option<Arc<RpcApiGrpc>>) -> Connection {
        Connection {
            inner: Arc::new(ConnectionInner {
                id,
                peer: *peer,
                messenger,
                grpc_api,
                listener_id: Mutex::new(None),
                subscriptions: Mutex::new(HashSet::new()),
            }),
        }
    }
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn messenger(&self) -> &Arc<Messenger> {
        &self.inner.messenger
    }

    pub fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.inner
            .grpc_api
            .as_ref()
            .cloned()
            .unwrap_or_else(|| panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references"))
    }

    pub fn listener_id(&self) -> Option<ListenerId> {
        *self.inner.listener_id.lock().unwrap()
    }

    pub fn register_notification_listener(&self, id: ListenerId) {
        self.inner.listener_id.lock().unwrap().replace(id);
    }

    pub fn subscriptions(&self) -> &Mutex<HashSet<ListenerId>> {
        &self.inner.subscriptions
    }

    pub fn drain_subscriptions(&self) -> Vec<ListenerId> {
        self.inner.subscriptions.lock().unwrap().drain().collect()
    }

    pub fn has_listener_id(&self, id: &u64) -> bool {
        self.inner.subscriptions.lock().unwrap().get(id).is_some()
    }

    pub fn peer(&self) -> &SocketAddr {
        &self.inner.peer
    }
}

pub type ConnectionReference = Arc<Connection>;
