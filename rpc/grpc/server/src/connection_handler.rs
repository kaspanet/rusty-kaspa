use crate::{
    collector::{GrpcServiceCollector, GrpcServiceConverter},
    connection::Connection,
    manager::{ManagerEvent, RegistrationRequest},
    request_handler::{factory::Factory, interface::Interface},
};
use futures::{FutureExt, Stream};
use kaspa_core::{debug, info, warn};
use kaspa_grpc_core::{
    protowire::{
        rpc_server::{Rpc, RpcServer},
        KaspadRequest, KaspadResponse,
    },
    RPC_MAX_MESSAGE_SIZE,
};
use kaspa_notify::{
    connection::ChannelType,
    events::EVENT_TYPE_ARRAY,
    listener::ListenerLifespan,
    notifier::Notifier,
    subscriber::Subscriber,
    subscription::{context::SubscriptionContext, MutationPolicies, UtxosChangedMutationPolicy},
};
use kaspa_rpc_core::{
    api::rpc::DynRpcService,
    notify::{channel::NotificationChannel, connection::ChannelConnection},
    Notification, RpcResult,
};
use kaspa_utils::networking::NetAddress;
use kaspa_utils_tower::{
    counters::TowerConnectionCounters,
    middleware::{measure_request_body_size_layer, CountBytesBody, MapResponseBodyLayer},
};
use std::fmt::Debug;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc::{channel as mpsc_channel, Sender as MpscSender};
use tokio::{
    sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender},
    time::timeout,
};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{codec::CompressionEncoding, transport::Server as TonicServer, Request, Response};

#[derive(Clone)]
pub struct ServerContext {
    /// The RPC core service API the RPC methods are calling
    pub core_service: DynRpcService,
    /// The notifier relaying RPC core notifications to connections
    pub notifier: Arc<Notifier<Notification, Connection>>,
}

impl ServerContext {
    pub fn new(core_service: DynRpcService, notifier: Arc<Notifier<Notification, Connection>>) -> Self {
        Self { core_service, notifier }
    }
}

impl Debug for ServerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerContext").finish()
    }
}

/// A protowire gRPC connections handler.
#[derive(Clone)]
pub struct ConnectionHandler {
    manager_sender: MpscSender<ManagerEvent>,
    server_context: ServerContext,
    interface: Arc<Interface>,
    running: Arc<AtomicBool>,
    counters: Arc<TowerConnectionCounters>,
}

const GRPC_SERVER: &str = "grpc-server";

impl ConnectionHandler {
    pub(crate) fn new(
        network_bps: u64,
        manager_sender: MpscSender<ManagerEvent>,
        core_service: DynRpcService,
        core_notifier: Arc<Notifier<Notification, ChannelConnection>>,
        subscription_context: SubscriptionContext,
        broadcasters: usize,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        // This notifier UTXOs subscription granularity to rpc-core notifier
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);

        // Prepare core objects
        let core_channel = NotificationChannel::default();
        let core_listener_id = core_notifier.register_new_listener(
            ChannelConnection::new(GRPC_SERVER, core_channel.sender(), ChannelType::Closable),
            ListenerLifespan::Static(policies),
        );

        // Prepare internals
        let core_events = EVENT_TYPE_ARRAY[..].into();
        let converter = Arc::new(GrpcServiceConverter::new());
        let collector = Arc::new(GrpcServiceCollector::new(GRPC_SERVER, core_channel.receiver(), converter));
        let subscriber = Arc::new(Subscriber::new(GRPC_SERVER, core_events, core_notifier, core_listener_id));
        let notifier: Arc<Notifier<Notification, Connection>> = Arc::new(Notifier::new(
            GRPC_SERVER,
            core_events,
            vec![collector],
            vec![subscriber],
            subscription_context,
            broadcasters,
            policies,
        ));
        let server_context = ServerContext::new(core_service, notifier);
        let interface = Arc::new(Factory::new_interface(server_context.clone(), network_bps));
        let running = Default::default();

        Self { manager_sender, server_context, interface, running, counters }
    }

    /// Launches a gRPC server listener loop
    pub(crate) fn serve(&self, serve_address: NetAddress) -> OneshotSender<()> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let (signal_sender, signal_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        info!("GRPC Server starting on: {}", serve_address);

        let bytes_tx = self.counters.bytes_tx.clone();
        let bytes_rx = self.counters.bytes_rx.clone();

        // Spawn server task
        let server_handle = tokio::spawn(async move {
            let protowire_server = RpcServer::new(connection_handler)
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip)
                .max_decoding_message_size(RPC_MAX_MESSAGE_SIZE);

            // TODO: check whether we should set tcp_keepalive
            // const GRPC_KEEP_ALIVE_PING_INTERVAL: Duration = Duration::from_secs(5);
            // const GRPC_KEEP_ALIVE_PING_TIMEOUT: Duration = Duration::from_secs(120);
            let serve_result = TonicServer::builder()
                // .http2_keepalive_interval(Some(GRPC_KEEP_ALIVE_PING_INTERVAL))
                // .http2_keepalive_timeout(Some(GRPC_KEEP_ALIVE_PING_TIMEOUT))
                .layer(measure_request_body_size_layer(bytes_rx, |b| b))
                .layer(MapResponseBodyLayer::new(move |body| CountBytesBody::new(body, bytes_tx.clone())))
                .add_service(protowire_server)
                .serve_with_shutdown(
                    serve_address.into(),
                    signal_receiver.map(|_| {
                        debug!("GRPC, Server received the shutdown signal");
                    }),
                )
                .await;

            match serve_result {
                Ok(_) => info!("GRPC Server stopped on: {}", serve_address),
                Err(err) => panic!("GRPC Server {serve_address} stopped with error: {err:?}"),
            }
        });

        // Spawn termination task
        tokio::spawn(async move {
            let _ = termination_receiver.await;
            signal_sender.send(()).expect("send signal");
            if (timeout(Duration::from_secs(1), server_handle).await).is_err() {
                warn!("GRPC Server stopped forcefully on: {}", serve_address);
            }
        });
        termination_sender
    }

    #[inline(always)]
    fn server_context(&self) -> ServerContext {
        self.server_context.clone()
    }

    #[inline(always)]
    fn interface(&self) -> Arc<Interface> {
        self.interface.clone()
    }

    #[inline(always)]
    fn manager_sender(&self) -> MpscSender<ManagerEvent> {
        self.manager_sender.clone()
    }

    #[inline(always)]
    fn notifier(&self) -> Arc<Notifier<Notification, Connection>> {
        self.server_context.notifier.clone()
    }

    pub fn start(&self) {
        debug!("GRPC, Starting the connection handler");

        // Start the internal notifier
        self.notifier().start();

        // Accept new incoming connections
        self.running.store(true, Ordering::SeqCst);
    }

    pub async fn stop(&self) -> RpcResult<()> {
        debug!("GRPC, Stopping the connection handler");

        // Refuse new incoming connections
        self.running.store(false, Ordering::SeqCst);

        // Wait for the internal notifier to stop
        // Note that this requires the core service it is listening to have closed its listener
        match timeout(Duration::from_millis(100), self.notifier().join()).await {
            Ok(_) => {
                debug!("GRPC, Stopped the connection handler");
            }
            Err(_) => {
                warn!("GRPC, Stopped the connection handler forcefully");
            }
        }

        Ok(())
    }

    pub fn outgoing_route_channel_size() -> usize {
        1024
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        debug!("GRPC, Dropping connection handler, refs {}", Arc::strong_count(&self.running));
    }
}

#[tonic::async_trait]
impl Rpc for ConnectionHandler {
    type MessageStreamStream = Pin<Box<dyn Stream<Item = Result<KaspadResponse, tonic::Status>> + Send + Sync + 'static>>;

    /// Handle the new arriving client connection
    async fn message_stream(
        &self,
        request: Request<tonic::Streaming<KaspadRequest>>,
    ) -> Result<Response<Self::MessageStreamStream>, tonic::Status> {
        const SERVICE_IS_DOWN: &str = "The gRPC service is down";

        if !self.running.load(Ordering::SeqCst) {
            return Err(tonic::Status::new(tonic::Code::Unavailable, SERVICE_IS_DOWN));
        }

        let remote_address = request.remote_addr().ok_or_else(|| {
            tonic::Status::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address")
        })?;

        debug!("GRPC, Incoming message stream from {:?}", remote_address);

        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_route_channel_size());
        let incoming_stream = request.into_inner();

        // Build the connection object
        let connection = Connection::new(
            remote_address,
            self.server_context(),
            self.interface(),
            self.manager_sender(),
            incoming_stream,
            outgoing_route,
        );

        // Try to get the connection registered into the central Manager
        let (register_sender, register_receiver) = oneshot_channel();
        match self.manager_sender.send(ManagerEvent::NewConnection(RegistrationRequest::new(connection, register_sender))).await {
            Ok(()) => match register_receiver.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    warn!("GRPC, refusing incoming message stream from {:?} - {}", remote_address, err);
                    return Err(tonic::Status::new(
                        tonic::Code::ResourceExhausted,
                        "The gRPC service has reached full capacity and accepts no new connection",
                    ));
                }
                Err(err) => {
                    debug!(
                        "GRPC, Refusing incoming message stream from {:?} - connection manager responded with {}",
                        remote_address, err
                    );
                    return Err(tonic::Status::new(tonic::Code::Unavailable, SERVICE_IS_DOWN));
                }
            },
            Err(err) => {
                debug!(
                    "GRPC, Refusing incoming message stream from {:?} - failed to contact connection manager, error {}",
                    remote_address, err
                );
                return Err(tonic::Status::new(tonic::Code::Unavailable, SERVICE_IS_DOWN));
            }
        }

        // Give tonic a receiver stream (messages sent to it will be forwarded to the client)
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver).map(Ok)) as Self::MessageStreamStream))
    }
}
