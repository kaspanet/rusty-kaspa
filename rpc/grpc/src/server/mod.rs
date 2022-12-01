use crate::protowire::rpc_server::RpcServer;
use kaspa_core::trace;
use kaspa_utils::triggers::DuplexTrigger;
use rpc_core::server::service::RpcCoreService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tonic::{codec::CompressionEncoding, transport::Server};

pub mod connection;
pub mod service;

pub type StatusResult<T> = Result<T, tonic::Status>;

pub struct GrpcServer {
    address: SocketAddr,
    grpc_service: Arc<service::GrpcService>,
    shutdown: DuplexTrigger,
}

impl GrpcServer {
    pub fn new(address: SocketAddr, core_service: Arc<RpcCoreService>) -> Self {
        let grpc_service = Arc::new(service::GrpcService::new(core_service));
        Self { address, grpc_service, shutdown: DuplexTrigger::default() }
    }

    pub fn start(self: &Arc<GrpcServer>) -> JoinHandle<()> {
        trace!("gRPC server listening on: {}", self.address);

        // Start the gRPC service
        let grpc_service = self.grpc_service.clone();
        grpc_service.start();

        // Create a protowire RPC server
        let svc = RpcServer::new(self.grpc_service.clone())
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip);

        // Prepare a shutdown future
        let shutdown_signal = self.shutdown.request.listener.clone();

        // Launch the tonic server and wait for it to shutdown
        let address = self.address;
        let shutdown_executed = self.shutdown.response.trigger.clone();
        tokio::spawn(async move {
            match Server::builder().add_service(svc).serve_with_shutdown(address, shutdown_signal).await {
                Ok(_) => {
                    trace!("gRPC server exited gracefully");
                }
                Err(err) => {
                    trace!("gRPC server exited with error {0}", err);
                }
            }
            shutdown_executed.trigger();
        })
    }

    pub fn signal_exit(self: &Arc<GrpcServer>) {
        self.shutdown.request.trigger.trigger();
    }

    pub fn stop(self: &Arc<GrpcServer>) -> JoinHandle<()> {
        // Launch the shutdown process as a task
        let shutdown_executed_signal = self.shutdown.response.listener.clone();
        let grpc_service = self.grpc_service.clone();
        tokio::spawn(async move {
            // Wait for the tonic server to gracefully shutdown
            shutdown_executed_signal.await;

            // Stop the gRPC service gracefully
            match grpc_service.stop().await {
                Ok(_) => {}
                Err(err) => {
                    trace!("Error while stopping the gRPC service: {0}", err);
                }
            }
            match grpc_service.finalize().await {
                Ok(_) => {}
                Err(err) => {
                    trace!("Error while finalizing the gRPC service: {0}", err);
                }
            }
        })
    }
}
