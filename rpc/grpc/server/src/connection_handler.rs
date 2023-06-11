use crate::{
    collector::{GrpcServiceCollector, GrpcServiceConverter},
    connection::{GrpcConnection, GrpcConnectionManager},
};
use futures::{FutureExt, Stream};
use kaspa_core::{debug, info};
use kaspa_grpc_core::{
    protowire::{
        rpc_server::{Rpc, RpcServer},
        KaspadRequest, KaspadResponse,
    },
    RPC_MAX_MESSAGE_SIZE,
};
use kaspa_notify::{events::EVENT_TYPE_ARRAY, listener::ListenerId, notifier::Notifier, subscriber::Subscriber};
use kaspa_rpc_core::{
    notify::{channel::NotificationChannel, connection::ChannelConnection},
    Notification, RpcResult,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::networking::NetAddress;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc::channel as mpsc_channel;
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{codec::CompressionEncoding, transport::Server as TonicServer, Request, Response};

/// A protowire gRPC connections handler.
pub struct GrpcConnectionHandler {
    core_service: Arc<RpcCoreService>,
    core_channel: NotificationChannel,
    core_listener_id: ListenerId,
    connection_manager: GrpcConnectionManager,
    notifier: Arc<Notifier<Notification, GrpcConnection>>,
    running: AtomicBool,
}

const GRPC_SERVER: &str = "grpc-server";

impl GrpcConnectionHandler {
    pub fn new(core_service: Arc<RpcCoreService>) -> Self {
        // Prepare core objects
        let core_channel = NotificationChannel::default();
        let core_listener_id = core_service.notifier().register_new_listener(ChannelConnection::new(core_channel.sender()));

        // Prepare internals
        let core_events = EVENT_TYPE_ARRAY[..].into();
        let converter = Arc::new(GrpcServiceConverter::new());
        let collector = Arc::new(GrpcServiceCollector::new(core_channel.receiver(), converter));
        let subscriber = Arc::new(Subscriber::new(core_events, core_service.notifier(), core_listener_id));
        let notifier: Arc<Notifier<Notification, GrpcConnection>> =
            Arc::new(Notifier::new(core_events, vec![collector], vec![subscriber], 10, GRPC_SERVER));
        let connection_manager = GrpcConnectionManager::new(Self::max_connections());

        Self { core_service, core_channel, core_listener_id, connection_manager, notifier, running: AtomicBool::new(false) }
    }

    /// Launches a gRPC server listener loop
    pub(crate) fn serve(self: &Arc<Self>, serve_address: NetAddress) -> OneshotSender<()> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        info!("gRPC Server starting on: {}", serve_address);
        tokio::spawn(async move {
            let protowire_server = RpcServer::from_arc(connection_handler.clone())
                .send_compressed(CompressionEncoding::Gzip)
                .accept_compressed(CompressionEncoding::Gzip)
                .max_decoding_message_size(RPC_MAX_MESSAGE_SIZE);

            // TODO: check whether we should set tcp_keepalive
            let serve_result = TonicServer::builder()
                .add_service(protowire_server)
                .serve_with_shutdown(serve_address.into(), termination_receiver.map(drop))
                .await;

            match serve_result {
                Ok(_) => info!("gRPC Server stopped: {}", serve_address),
                Err(err) => panic!("gRPC Server {serve_address} stopped with error: {err:?}"),
            }
        });
        termination_sender
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, GrpcConnection>> {
        self.notifier.clone()
    }

    pub fn start(&self) {
        debug!("gRPC: Starting the connection handler");

        // Start the internal notifier
        self.notifier().start();

        // Accept new incoming connections
        self.running.store(true, Ordering::SeqCst);
    }

    pub async fn stop(&self) -> RpcResult<()> {
        debug!("gRPC: Stopping the connection handler");

        // Refuse new incoming connections
        self.running.store(false, Ordering::SeqCst);

        // Close all existing connections
        self.connection_manager.terminate_all_connections();

        // Unregister from the core service notifier
        self.core_service.notifier().unregister_listener(self.core_listener_id)?;
        self.core_channel.receiver().close();

        // Stop the internal notifier
        self.notifier().stop().await?;

        Ok(())
    }

    pub fn max_connections() -> usize {
        24
    }

    pub fn outgoing_route_channel_size() -> usize {
        128
    }
}

#[tonic::async_trait]
impl Rpc for GrpcConnectionHandler {
    type MessageStreamStream = Pin<Box<dyn Stream<Item = Result<KaspadResponse, tonic::Status>> + Send + Sync + 'static>>;

    async fn message_stream(
        &self,
        request: Request<tonic::Streaming<KaspadRequest>>,
    ) -> Result<Response<Self::MessageStreamStream>, tonic::Status> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(tonic::Status::new(tonic::Code::Unavailable, "The gRPC service is down".to_string()));
        }

        let remote_address = request.remote_addr().ok_or_else(|| {
            tonic::Status::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address".to_string())
        })?;

        if self.connection_manager.is_full() {
            return Err(tonic::Status::new(
                tonic::Code::PermissionDenied,
                "The gRPC service has reached full capacity and accepts no new connection".to_string(),
            ));
        }

        debug!("gRPC: incoming message stream from {:?}", remote_address);

        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_route_channel_size());
        let incoming_stream = request.into_inner();

        // Build the connection object & register it
        let connection = GrpcConnection::new(
            remote_address,
            self.core_service.clone(),
            self.connection_manager.clone(),
            self.notifier(),
            incoming_stream,
            outgoing_route,
        );
        self.connection_manager.register(connection);

        // Return connection stream
        let response_stream = ReceiverStream::new(outgoing_receiver);
        Ok(Response::new(Box::pin(response_stream)))
    }
}
