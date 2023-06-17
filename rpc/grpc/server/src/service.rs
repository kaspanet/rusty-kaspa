use crate::adaptor::Adaptor;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::{networking::NetAddress, triggers::SingleTrigger};
use std::sync::Arc;

const GRPC_SERVICE: &str = "grpc-service";

pub struct GrpcService {
    net_address: NetAddress,
    core_service: Arc<RpcCoreService>,
    rpc_max_clients: usize,
    shutdown: SingleTrigger,
}

impl GrpcService {
    pub fn new(address: NetAddress, core_service: Arc<RpcCoreService>, rpc_max_clients: usize) -> Self {
        Self { net_address: address, core_service, rpc_max_clients, shutdown: SingleTrigger::default() }
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

        let grpc_adaptor =
            Adaptor::server(self.net_address, self.core_service.clone(), self.core_service.notifier(), self.rpc_max_clients);

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // Keep the gRPC server running until a service shutdown signal is received
            shutdown_signal.await;

            // Stop the connection handler, closing all connections and refusing new ones
            match grpc_adaptor.terminate().await {
                Ok(_) => {}
                Err(err) => {
                    warn!("{} error while stopping the connection handler: {}", GRPC_SERVICE, err);
                }
            }
            // On exit, the adaptor is dropped, causing the server termination
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
