use crate::protowire::rpc_server::RpcServer;
use kaspa_core::core::Core;
use kaspa_core::service::Service;
use kaspa_core::trace;
use kaspa_utils::channel::Channel;
use rpc_core::server::service::RpcCoreService;
use std::net::SocketAddr;
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};
use tonic::{codec::CompressionEncoding, transport::Server};

pub mod connection;
pub mod service;

pub type StatusResult<T> = Result<T, tonic::Status>;

const GRPC_SERVER: &str = "grpc-server";

pub struct GrpcServer {
    address: SocketAddr,
    grpc_service: Arc<service::GrpcService>,
    shutdown_channel: Channel<()>,
}

impl GrpcServer {
    pub fn new(address: SocketAddr, core_service: Arc<RpcCoreService>) -> Self {
        let grpc_service = Arc::new(service::GrpcService::new(core_service));
        let shutdown_channel = Channel::default();
        Self { address, grpc_service, shutdown_channel }
    }

    pub fn init(self: Arc<GrpcServer>) -> Vec<JoinHandle<()>> {
        vec![thread::Builder::new().name(GRPC_SERVER.to_string()).spawn(move || self.worker()).unwrap()]
    }

    // TODO: In the future, we might group all async flows under one worker and join them.
    //       For now, this is not necessary since we only have one.

    #[tokio::main]
    pub async fn worker(self: Arc<GrpcServer>) {
        trace!("gRPC server listening on: {}", self.address);

        // Start the gRPC service
        let grpc_service = self.grpc_service.clone();
        grpc_service.start();

        // Prepare a shutdown channel and a shutdown future
        let shutdown_rx = self.shutdown_channel.receiver();
        let shutdown_signal = async move {
            shutdown_rx.recv().await.unwrap();
            grpc_service.stop().await.unwrap();
            grpc_service.finalize().await.unwrap();
        };

        // Create a protowire RPC server
        let svc = RpcServer::new(self.grpc_service.clone())
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip);

        // Launch the tonic server and wait for it to shutdown
        Server::builder().add_service(svc).serve_with_shutdown(self.address, shutdown_signal).await.unwrap();
    }

    pub fn signal_exit(self: Arc<GrpcServer>) {
        // TODO: investigate a better signaling strategy
        self.shutdown_channel.sender().send_blocking(()).unwrap();
    }

    pub fn shutdown(self: Arc<GrpcServer>, wait_handles: Vec<JoinHandle<()>>) {
        self.signal_exit();
        // Wait for async gRPC server to exit
        for handle in wait_handles {
            handle.join().unwrap();
        }
    }
}

impl Service for GrpcServer {
    fn ident(self: Arc<GrpcServer>) -> &'static str {
        GRPC_SERVER
    }

    fn start(self: Arc<GrpcServer>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<GrpcServer>) {
        self.signal_exit()
    }
}
