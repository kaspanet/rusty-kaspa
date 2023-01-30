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
    // pub peer: SocketAddr,
    // pub messenger: Arc<Messenger>,
    // pub id: u64,
    // pub messenger: Arc<Messenger>,
    // pub grpc_api: Option<Arc<RpcApiGrpc>>,
    // pub notification_listener_id: Option<ListenerId>,
}

impl Connection {
    pub fn new(id: u64, peer: &SocketAddr, messenger: Arc<Messenger>, grpc_api: Option<Arc<RpcApiGrpc>>) -> Connection {
        Connection { inner: Arc::new(ConnectionInner { id, messenger, grpc_api, listener_id: Mutex::new(None) }) }
        // Connection { id, messenger, grpc_api, notification_listener_id: None }
    }
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn messenger(&self) -> &Arc<Messenger> {
        //panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
        &self.inner.messenger //.clone()
    }

    pub fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.inner.grpc_api.as_ref().cloned().unwrap()
        // panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
    }

    // pub fn listener_id(&self) -> Option<ListenerReceiverSide> {
    pub fn listener_id(&self) -> Option<ListenerId> {
        // &self.inner.listener.lock().unwrap().as_ref().map(|l|l.id)
        *self.inner.listener_id.lock().unwrap()
    }

    // pub fn register_notification_listener(&self, id : ListenerId, receiver : Receiver<Arc<NotificationMessage>>) {
    pub fn register_notification_listener(&self, id: ListenerId) {
        self.inner.listener_id.lock().unwrap().replace(id);
    }
}

pub type ConnectionReference = Arc<Connection>;

// impl Connection {
//     pub fn new(peer: &SocketAddr, messenger: Arc<Messenger>) -> Self {
//         ConnectionContext { peer: *peer, messenger }
//     }
// }
// type ConnectionContextReference = Arc<ConnectionContext>;

// impl RpcApiContainer for Connection {
//     fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
//         panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
//     }
// }

// impl MessengerContainer for Connection {
//     fn get_messenger(&self) -> Arc<Messenger> {
//         //panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
//         self.inner.messenger.clone()
//     }
// }
