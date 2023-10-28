use crate::{
    error::{GrpcServerError, GrpcServerResult},
    manager::ManagerEvent,
    request_handler::handler_factory::HandlerFactory,
};
use kaspa_core::{debug, info, trace};
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use kaspa_notify::{
    connection::Connection as ConnectionT, error::Error as NotificationError, listener::ListenerId, notifier::Notifier,
};
use kaspa_rpc_core::{api::rpc::DynRpcService, Notification};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Display,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio::{select, sync::mpsc::error::TrySendError};
use tonic::Streaming;
use uuid::Uuid;

pub type IncomingRoute = MpscReceiver<KaspadRequest>;
pub type GrpcNotifier = Notifier<Notification, Connection>;
pub type GrpcSender = MpscSender<KaspadResponse>;
pub type StatusResult<T> = Result<T, tonic::Status>;
pub type ConnectionId = Uuid;

type RequestSender = MpscSender<KaspadRequest>;
type RoutingMap = HashMap<KaspadPayloadOps, RequestSender>;

#[derive(Debug, Default)]
struct InnerMutableState {
    /// Used on connection close to signal the connection receive loop to exit
    shutdown_signal: Option<OneshotSender<()>>,

    /// Notification listener Id
    ///
    /// Registered when handling the first subscription to any notifications
    listener_id: Option<ListenerId>,
}

impl InnerMutableState {
    fn new(shutdown_signal: Option<OneshotSender<()>>) -> Self {
        Self { shutdown_signal, ..Default::default() }
    }
}

#[derive(Debug)]
struct Inner {
    connection_id: ConnectionId,

    /// The socket address of this client
    net_address: SocketAddr,

    /// The outgoing route for sending messages to this client
    outgoing_route: GrpcSender,

    /// Routing map for mapping messages to RPC op handlers
    routing_map: RwLock<RoutingMap>,

    /// A channel sender for internal event management.
    /// Used to send information from each router to a central manager object
    manager_sender: MpscSender<ManagerEvent>,

    /// The notifier relaying consensus notifications to connections
    notifier: Arc<GrpcNotifier>,

    /// Used for managing connection mutable state
    mutable_state: Mutex<InnerMutableState>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        debug!("GRPC: dropping connection {}", self.connection_id);
    }
}

#[derive(Clone, Debug)]
pub struct Connection {
    inner: Arc<Inner>,
}

impl Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.inner.connection_id, self.inner.net_address)
    }
}

impl Connection {
    pub(crate) fn new(
        net_address: SocketAddr,
        core_service: DynRpcService,
        manager_sender: MpscSender<ManagerEvent>,
        notifier: Arc<Notifier<Notification, Connection>>,
        mut incoming_stream: Streaming<KaspadRequest>,
        outgoing_route: GrpcSender,
    ) -> Self {
        let (shutdown_sender, mut shutdown_receiver) = oneshot_channel();
        let connection = Self {
            inner: Arc::new(Inner {
                connection_id: Uuid::new_v4(),
                net_address,
                outgoing_route,
                routing_map: Default::default(),
                manager_sender,
                notifier: notifier.clone(),
                mutable_state: Mutex::new(InnerMutableState::new(Some(shutdown_sender))),
            }),
        };
        let connection_clone = connection.clone();
        // Start the connection receive loop
        debug!("GRPC: Connection starting for client {}", connection);
        tokio::spawn(async move {
            loop {
                select! {
                    biased; // We use biased polling so that the shutdown signal is always checked first

                    _ = &mut shutdown_receiver => {
                        debug!("GRPC: Connection receive loop - shutdown signal received, exiting connection receive loop, client: {}", connection.identity());
                        break;
                    }

                    res = incoming_stream.message() => match res {
                        Ok(Some(request)) => {
                            trace!("GRPC: request: {:?}, client: {}", request, connection.identity());
                            match connection.route_to_handler(&core_service, request).await {
                                Ok(()) => {},
                                Err(e) => {
                                    debug!("GRPC: Connection receive loop - route error: {} for client: {}", e, connection);
                                    break;
                                }
                            }
                        }
                        Ok(None) => {
                            info!("GRPC, incoming stream ended from client {}", connection);
                            break;
                        }
                        Err(status) => {
                            if let Some(err) = match_for_io_error(&status) {
                                info!("GRPC, network error: {} from client {}", err, connection);
                            } else {
                                info!("GRPC, network error: {} from client {}", status, connection);
                            }
                            break;
                        }
                    }
                }
            }
            // Mark as closed
            connection.close();

            let connection_id = connection.to_string();
            let inner = Arc::downgrade(&connection.inner);
            drop(connection);

            debug!("GRPC: Connection receive loop - exited, client: {}, client refs: {}", connection_id, inner.strong_count());
        });

        connection_clone
    }

    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        Arc::ptr_eq(&this.inner, &other.inner)
    }

    pub fn net_address(&self) -> SocketAddr {
        self.inner.net_address
    }

    pub fn identity(&self) -> ConnectionId {
        self.inner.connection_id
    }

    pub fn notifier(&self) -> Arc<GrpcNotifier> {
        self.inner.notifier.clone()
    }

    pub fn get_or_register_listener_id(&self) -> ListenerId {
        *self
            .inner
            .mutable_state
            .lock()
            .listener_id
            .get_or_insert_with(|| self.inner.notifier.as_ref().register_new_listener(self.clone()))
    }

    fn unregister_listener(&self) {
        let listener_id = self.inner.mutable_state.lock().listener_id.take();
        if let Some(listener_id) = listener_id {
            self.inner.notifier.unregister_listener(listener_id).expect("unregister listener")
        }
    }

    pub fn request_channel_size() -> usize {
        256
    }

    fn subscribe(&self, core_service: &DynRpcService, rpc_op: KaspadPayloadOps) -> RequestSender {
        match self.inner.routing_map.write().entry(rpc_op) {
            Entry::Vacant(entry) => {
                let (sender, receiver) = mpsc_channel(Self::request_channel_size());
                let handler = HandlerFactory::new_handler(rpc_op, self.clone(), core_service, self.inner.notifier.clone(), receiver);
                handler.launch();
                entry.insert(sender.clone());
                trace!("GRPC, Connection::subscribe - {:?} route is registered, client:{:?}", rpc_op, self.identity());
                sender
            }
            Entry::Occupied(entry) => entry.get().clone(),
        }
    }

    async fn route_to_handler(&self, core_service: &DynRpcService, request: KaspadRequest) -> GrpcServerResult<()> {
        // TODO: add appropriate error
        if request.payload.is_none() {
            debug!("GRPC, Route to handler got empty payload, client: {}", self);
            return Err(GrpcServerError::InvalidRequestPayload);
        }
        let rpc_op = request.payload.as_ref().unwrap().into();
        let sender = self.inner.routing_map.read().get(&rpc_op).cloned();
        let sender = sender.unwrap_or_else(|| self.subscribe(core_service, rpc_op));
        match sender.send(request).await {
            Ok(_) => Ok(()),
            Err(_) => Err(GrpcServerError::ClosedHandler(rpc_op)),
        }
    }

    /// Enqueues a response to be sent to the client
    pub async fn enqueue(&self, response: KaspadResponse) -> GrpcServerResult<()> {
        assert!(response.payload.is_some(), "Kaspad gRPC message should always have a value");
        match self.inner.outgoing_route.try_send(response) {
            Ok(_) => Ok(()),
            Err(TrySendError::Closed(_)) => Err(GrpcServerError::ConnectionClosed),
            Err(TrySendError::Full(_)) => Err(GrpcServerError::OutgoingRouteCapacityReached(self.to_string())),
        }
    }
}

fn match_for_io_error(err_status: &tonic::Status) -> Option<&std::io::Error> {
    let mut err: &(dyn std::error::Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default)]
pub enum GrpcEncoding {
    #[default]
    ProtowireResponse = 0,
}

impl ConnectionT for Connection {
    type Notification = Notification;
    type Message = Arc<KaspadResponse>;
    type Encoding = GrpcEncoding;
    type Error = super::error::GrpcServerError;

    fn encoding(&self) -> Self::Encoding {
        GrpcEncoding::ProtowireResponse
    }

    fn into_message(notification: &kaspa_rpc_core::Notification, _: &Self::Encoding) -> Self::Message {
        Arc::new((notification).into())
    }

    fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.inner.outgoing_route.try_send((*message).clone())?),
            false => Err(NotificationError::ConnectionClosed.into()),
        }
    }

    /// Closes the connection, signals exit, and cleans up all resources so that underlying connections will be aborted correctly.
    ///
    /// Returns true of this is the first call to close.
    fn close(&self) -> bool {
        // Acquire state mutex and send the shutdown signal
        // NOTE: Using a block to drop the lock asap
        {
            let mut state = self.inner.mutable_state.lock();

            if let Some(signal) = state.shutdown_signal.take() {
                let _ = signal.send(());
            } else {
                // This means the connection was already closed
                trace!("GRPC: Connection close was called more than once, client: {}", self);
                return false;
            }
        }
        // Unregister from notifier
        self.unregister_listener();

        // Drop all routes, triggering the drop of all handlers
        self.inner.routing_map.write().clear();

        // Send a close notification to the central Manager
        self.inner
            .manager_sender
            .try_send(ManagerEvent::ConnectionClosing(self.clone()))
            .expect("manager receiver should never drop before senders");

        true
    }

    fn is_closed(&self) -> bool {
        self.inner.mutable_state.lock().shutdown_signal.is_none()
    }
}
