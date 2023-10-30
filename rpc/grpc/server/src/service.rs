use crate::{adaptor::Adaptor, manager::Manager};
use kaspa_core::{
    debug,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::tcp_limiter::Limit;
use kaspa_utils::{networking::NetAddress, triggers::SingleTrigger};
use std::sync::Arc;

const GRPC_SERVICE: &str = "grpc-service";

pub struct GrpcService {
    net_address: NetAddress,
    core_service: Arc<RpcCoreService>,
    rpc_max_clients: usize,
    shutdown: SingleTrigger,
    tcp_limit: Option<Arc<Limit>>,
}

impl GrpcService {
    pub fn new(address: NetAddress, core_service: Arc<RpcCoreService>, rpc_max_clients: usize, tcp_limit: Option<Arc<Limit>>) -> Self {
        Self { net_address: address, core_service, rpc_max_clients, shutdown: Default::default(), tcp_limit }
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

        let manager = Manager::new(self.rpc_max_clients);
        let grpc_adaptor = Adaptor::server(
            self.net_address,
            manager,
            self.core_service.clone(),
            self.core_service.notifier(),
            self.tcp_limit.clone(),
        );

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // Keep the gRPC server running until a service shutdown signal is received
            shutdown_signal.await;

            // Stop the connection handler, closing all connections and refusing new ones
            match grpc_adaptor.stop().await {
                Ok(_) => {
                    debug!("GRPC, Adaptor terminated successfully");
                }
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
        Box::pin(async move {
            trace!("{} stopped", GRPC_SERVICE);
            Ok(())
        })
    }
}
