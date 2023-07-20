use crate::{
    collector::{GrpcServiceCollector, GrpcServiceConverter},
    connection::Connection,
    manager::Manager,
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
use kaspa_notify::{connection::ChannelType, events::EVENT_TYPE_ARRAY, notifier::Notifier, subscriber::Subscriber};
use kaspa_rpc_core::{
    api::rpc::DynRpcService,
    notify::{channel::NotificationChannel, connection::ChannelConnection},
    Notification, RpcResult,
};
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
pub struct ConnectionHandler {
    core_service: DynRpcService,
    manager: Manager,
    notifier: Arc<Notifier<Notification, Connection>>,
    running: AtomicBool,
}

const GRPC_SERVER: &str = "grpc-server";

impl ConnectionHandler {
    pub fn new(core_service: DynRpcService, core_notifier: Arc<Notifier<Notification, ChannelConnection>>, manager: Manager) -> Self {
        // Prepare core objects
        let core_channel = NotificationChannel::default();
        let core_listener_id =
            core_notifier.register_new_listener(ChannelConnection::new(core_channel.sender(), ChannelType::Closable));

        // Prepare internals
        let core_events = EVENT_TYPE_ARRAY[..].into();
        let converter = Arc::new(GrpcServiceConverter::new());
        let collector = Arc::new(GrpcServiceCollector::new(GRPC_SERVER, core_channel.receiver(), converter));
        let subscriber = Arc::new(Subscriber::new(GRPC_SERVER, core_events, core_notifier, core_listener_id));
        let notifier: Arc<Notifier<Notification, Connection>> =
            Arc::new(Notifier::new(GRPC_SERVER, core_events, vec![collector], vec![subscriber], 10));

        Self { core_service, manager, notifier, running: AtomicBool::new(false) }
    }

    /// Launches a gRPC server listener loop
    pub(crate) fn serve(self: &Arc<Self>, serve_address: NetAddress) -> OneshotSender<()> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        info!("GRPC Server starting on: {}", serve_address);
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
                Ok(_) => info!("GRPC Server stopped on: {}", serve_address),
                Err(err) => panic!("gRPC Server {serve_address} stopped with error: {err:?}"),
            }
        });
        termination_sender
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, Connection>> {
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

        // Wait for the internal notifier to stop
        // Note that this requires the core service it is listening to to have closed it's listener
        self.notifier().join().await?;

        // Close all existing connections
        self.manager.terminate_all_connections();

        Ok(())
    }

    pub fn outgoing_route_channel_size() -> usize {
        128
    }
}

#[tonic::async_trait]
impl Rpc for ConnectionHandler {
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

        if self.manager.is_full() {
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
        let connection = Connection::new(
            remote_address,
            self.core_service.clone(),
            self.manager.clone(),
            self.notifier(),
            incoming_stream,
            outgoing_route,
        );
        self.manager.register(connection);

        // Return connection stream
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver))))
    }
}
