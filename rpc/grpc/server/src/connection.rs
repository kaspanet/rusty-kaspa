use crate::{
    error::{GrpcServerError, GrpcServerResult},
    manager::Manager,
    request_handler::handler_factory::HandlerFactory,
};
use kaspa_core::{debug, error, info, trace};
use kaspa_grpc_core::protowire::{KaspadRequest, KaspadResponse};
use kaspa_notify::{
    connection::Connection as ConnectionT, error::Error as NotificationError, listener::ListenerId, notifier::Notifier,
};
use kaspa_rpc_core::{
    api::{ops::RpcApiOps, rpc::DynRpcService},
    Notification,
};
use parking_lot::{Mutex, RwLock};
use std::{collections::HashMap, fmt::Display, net::SocketAddr, sync::Arc};
use tokio::select;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tonic::Streaming;
use uuid::Uuid;

pub type IncomingRoute = MpscReceiver<KaspadRequest>;
pub type GrpcNotifier = Notifier<Notification, Connection>;
pub type GrpcSender = MpscSender<KaspadResponse>;
pub type StatusResult<T> = Result<T, tonic::Status>;
pub type ConnectionId = Uuid;

type RequestSender = MpscSender<KaspadRequest>;
type RoutingMap = HashMap<RpcApiOps, RequestSender>;

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

    /// The manager of active connections
    manager: Manager,

    /// The notifier relaying consensus notifications to connections
    notifier: Arc<GrpcNotifier>,

    /// Used for managing connection mutable state
    mutable_state: Mutex<InnerMutableState>,
}

// impl Drop for Inner {
//     fn drop(&mut self) {
//         debug!("GRPC: dropping connection for client {}", self.connection_id);
//     }
// }

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
    pub fn new(
        net_address: SocketAddr,
        core_service: DynRpcService,
        manager: Manager,
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
                manager,
                notifier: notifier.clone(),
                mutable_state: Mutex::new(InnerMutableState::new(Some(shutdown_sender))),
            }),
        };
        let connection_clone = connection.clone();
        // Start the connection receive loop
        debug!("GRPC: Connection starting for client {}", connection);
        tokio::spawn(async move {
            // Do not preallocate some capacity because we do not expect so many different ops to be called
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
            connection.unregister_listener();
            connection.close();
            debug!(
                "GRPC: Connection receive loop - exited, client: {}, client refs: {}",
                connection,
                Arc::strong_count(&connection.inner)
            );
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

    fn register_listener(&self) -> ListenerId {
        self.inner.notifier.as_ref().register_new_listener(self.clone())
    }

    pub fn listener_id(&self) -> ListenerId {
        *self.inner.mutable_state.lock().listener_id.get_or_insert_with(|| self.register_listener())
    }

    fn unregister_listener(&self) {
        self.inner.mutable_state.lock().listener_id.take().map(|listener_id| self.inner.notifier.unregister_listener(listener_id));
    }

    pub fn request_channel_size() -> usize {
        256
    }

    fn subscribe(&self, core_service: &DynRpcService, rpc_op: RpcApiOps) -> RequestSender {
        let (sender, receiver) = mpsc_channel(Self::request_channel_size());
        let handler = HandlerFactory::new_handler(rpc_op, self.clone(), core_service, self.inner.notifier.clone(), receiver);
        handler.launch();
        match self.inner.routing_map.write().insert(rpc_op, sender.clone()) {
            Some(_) => {
                // Overrides an existing route -- panic
                error!("GRPC, Connection::subscribe overrides an existing value: {:?}, client: {}", rpc_op, self.identity());
                panic!("GRPC, Tried to replace an existing route");
            }
            None => {
                trace!("GRPC, Connection::subscribe - {:?} route is registered, client:{:?}", rpc_op, self.identity());
            }
        }
        sender
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
    pub async fn enqueue(&self, response: KaspadResponse) -> bool {
        assert!(response.payload.is_some(), "Kaspad gRPC message should always have a value");
        self.inner.outgoing_route.send(response).await.is_ok()
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

    fn close(&self) -> bool {
        if let Some(signal) = self.inner.mutable_state.lock().shutdown_signal.take() {
            let _ = signal.send(());
        } else {
            // This means the connection was already closed.
            // The typical case is the manager terminating all connections.
            return false;
        }

        // Drop all handler senders
        self.inner.routing_map.write().clear();

        self.inner.manager.unregister(self.clone());
        true
    }

    fn is_closed(&self) -> bool {
        self.inner.mutable_state.lock().shutdown_signal.is_none()
    }
}
