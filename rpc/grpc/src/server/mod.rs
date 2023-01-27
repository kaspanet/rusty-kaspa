use crate::protowire::rpc_server::RpcServer;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use kaspa_utils::triggers::DuplexTrigger;
use rpc_core::server::service::RpcCoreService;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::{codec::CompressionEncoding, transport::Server};

pub mod connection;
pub mod service;

pub type StatusResult<T> = Result<T, tonic::Status>;

const GRPC_SERVER: &str = "grpc-server";

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
}

impl AsyncService for GrpcServer {
    fn ident(self: Arc<Self>) -> &'static str {
        GRPC_SERVER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", GRPC_SERVER);

        let grpc_service = self.grpc_service.clone();
        let address = self.address;

        // Prepare a start shutdown signal receiver and a shutdown ended signal sender
        let shutdown_signal = self.shutdown.request.listener.clone();
        let shutdown_executed = self.shutdown.response.trigger.clone();

        // Return a future launching the tonic server and waiting for it to shutdown
        Box::pin(async move {
            // Start the gRPC service
            grpc_service.start();

            // Create a protowire RPC server
            let svc = RpcServer::new(self.grpc_service.clone())
                .send_compressed(CompressionEncoding::Gzip)
                .accept_compressed(CompressionEncoding::Gzip);

            // Start the tonic gRPC server
            trace!("gRPC server listening on: {}", address);
            let result = Server::builder()
                .add_service(svc)
                .serve_with_shutdown(address, shutdown_signal)
                .await
                .map_err(|err| AsyncServiceError::Service(format!("gRPC server exited with error `{}`", err)));

            if result.is_ok() {
                trace!("gRPC server exited gracefully");
            }

            // Send a signal telling the shutdown is done
            shutdown_executed.trigger();
            result
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", GRPC_SERVER);
        self.shutdown.request.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", GRPC_SERVER);
        // Launch the shutdown process as a task
        let shutdown_executed_signal = self.shutdown.response.listener.clone();
        let grpc_service = self.grpc_service.clone();
        Box::pin(async move {
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
            trace!("{} exiting", GRPC_SERVER);

            // TODO - review error handling
            Ok(())
        })
    }
}
