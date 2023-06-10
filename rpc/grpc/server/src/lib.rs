use connection_handler::GrpcConnectionHandler;
use kaspa_core::{
    debug,
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::{networking::NetAddress, triggers::SingleTrigger};
use std::sync::Arc;

pub mod collector;
pub mod connection;
pub mod connection_handler;
pub mod error;

pub type StatusResult<T> = Result<T, tonic::Status>;

const GRPC_SERVICE: &str = "grpc-service";

pub struct GrpcService {
    net_address: NetAddress,
    connection_handler: Arc<GrpcConnectionHandler>,
    shutdown: SingleTrigger,
}

impl GrpcService {
    pub fn new(address: NetAddress, core_service: Arc<RpcCoreService>) -> Self {
        let connection_handler = Arc::new(GrpcConnectionHandler::new(core_service));
        Self { net_address: address, connection_handler, shutdown: SingleTrigger::default() }
    }
}

impl AsyncService for GrpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        GRPC_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", GRPC_SERVICE);

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        let connection_handler = self.connection_handler.clone();
        let serve_address = self.net_address;
        let server_shutdown = connection_handler.serve(serve_address);
        connection_handler.start();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // Keep the gRPC server running until a service shutdown signal is received
            shutdown_signal.await;

            // Stop the connection handler, closing all connections and refusing new ones
            match connection_handler.stop().await {
                Ok(_) => {}
                Err(err) => {
                    debug!("gRPC: Error while stopping the connection handler: {0}", err);
                }
            }

            // Stop the gRPC server
            let _ = server_shutdown.send(());

            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", GRPC_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", GRPC_SERVICE);
        Box::pin(async move {
            trace!("{} exiting", GRPC_SERVICE);
            Ok(())
        })
    }
}
