use crate::StatusResult;
use kaspa_core::trace;
use kaspa_grpc_core::protowire::KaspadResponse;
use kaspa_rpc_core::notify::{
    connection::{Connection, Invariant},
    error::Error as NotificationError,
    listener::ListenerId,
    notifier::Notifier,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc::Sender;

pub type GrpcSender = Sender<StatusResult<KaspadResponse>>;

#[derive(Debug)]
struct Inner {
    pub address: SocketAddr,
    pub sender: GrpcSender,
    pub closed: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct GrpcConnection {
    inner: Arc<Inner>,
}

impl GrpcConnection {
    pub fn new(address: SocketAddr, sender: GrpcSender) -> Self {
        Self { inner: Arc::new(Inner { address, sender, closed: AtomicBool::new(false) }) }
    }
}

impl Connection for GrpcConnection {
    type Message = Arc<StatusResult<KaspadResponse>>;
    type Variant = Invariant;
    type Error = super::error::Error;

    fn variant(&self) -> Self::Variant {
        Invariant::Default
    }

    fn into_message(notification: &Arc<kaspa_rpc_core::Notification>, _: &Self::Variant) -> Self::Message {
        Arc::new(Ok((&**notification).into()))
    }

    fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.inner.sender.try_send((*message).clone())?),
            false => Err(NotificationError::ConnectionClosed.into()),
        }
    }

    fn close(&self) -> bool {
        // FIXME: actually close sender
        self.inner.closed.store(true, Ordering::SeqCst);
        true
    }

    fn is_closed(&self) -> bool {
        self.inner.sender.is_closed() || self.inner.closed.load(Ordering::SeqCst)
    }
}

pub(crate) struct GrpcConnectionManager {
    connections: HashMap<SocketAddr, GrpcConnection>,
    notifier: Arc<Notifier<GrpcConnection>>,
}

impl GrpcConnectionManager {
    pub fn new(notifier: Arc<Notifier<GrpcConnection>>) -> Self {
        Self { connections: HashMap::new(), notifier }
    }

    pub fn register(&mut self, address: SocketAddr, sender: GrpcSender) -> ListenerId {
        let connection = GrpcConnection::new(address, sender);
        let id = self.notifier.clone().register_new_listener(connection.clone());
        trace!("registering a new gRPC connection from: {address} with listener id {id}");

        // A pre-existing connection with same address is ignored here
        // TODO: see if some close pattern can be applied to the replaced connection
        self.connections.insert(address, connection);
        id
    }

    pub fn unregister(&mut self, address: SocketAddr) {
        if let Some(connection) = self.connections.remove(&address) {
            trace!("dismiss a gRPC connection from: {}", connection.inner.address);
            connection.close();
        }
    }
}
