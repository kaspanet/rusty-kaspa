use crate::notifications::*;
use rpc_core::api::rpc::RpcApi;
use rpc_grpc::client::RpcApiGrpc;
use std::sync::Arc;
use std::sync::Mutex;
use workflow_rpc::server::prelude::*;

#[derive(Debug)]
pub struct ConnectionInner {
    pub id: u64,
    pub messenger: Arc<Messenger>,
    pub grpc_api: Option<Arc<RpcApiGrpc>>,
    // not using an atomic in case an Id will change type in the future...
    pub listener_id: Mutex<Option<ListenerId>>,
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
    pub fn new(id: u64, _peer: &SocketAddr, messenger: Arc<Messenger>, grpc_api: Option<Arc<RpcApiGrpc>>) -> Connection {
        Connection { inner: Arc::new(ConnectionInner { id, messenger, grpc_api, listener_id: Mutex::new(None) }) }
    }
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn messenger(&self) -> &Arc<Messenger> {
        &self.inner.messenger
    }

    pub fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.inner.grpc_api.as_ref().cloned().unwrap_or_else(||{
            panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
        })
    }

    pub fn listener_id(&self) -> Option<ListenerId> {
        *self.inner.listener_id.lock().unwrap()
    }

    pub fn register_notification_listener(&self, id: ListenerId) {
        self.inner.listener_id.lock().unwrap().replace(id);
    }
}

pub type ConnectionReference = Arc<Connection>;
