use crate::{adaptor::Adaptor, manager::Manager};
use kaspa_consensus_core::config::Config;
use kaspa_core::{
    debug,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::{networking::NetAddress, triggers::SingleTrigger};
use kaspa_utils_tower::counters::TowerConnectionCounters;
use std::sync::Arc;
use triggered::Listener;

pub struct GrpcService {
    net_address: NetAddress,
    config: Arc<Config>,
    core_service: Arc<RpcCoreService>,
    rpc_max_clients: usize,
    broadcasters: usize,
    started: SingleTrigger,
    shutdown: SingleTrigger,
    counters: Arc<TowerConnectionCounters>,
}

impl GrpcService {
    pub const IDENT: &'static str = "grpc-service";

    pub fn new(
        address: NetAddress,
        config: Arc<Config>,
        core_service: Arc<RpcCoreService>,
        rpc_max_clients: usize,
        broadcasters: usize,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        Self {
            net_address: address,
            config,
            core_service,
            rpc_max_clients,
            broadcasters,
            started: Default::default(),
            shutdown: Default::default(),
            counters,
        }
    }

    pub fn started(&self) -> Listener {
        self.started.listener.clone()
    }
}

impl AsyncService for GrpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        Self::IDENT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", Self::IDENT);

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        let manager = Manager::new(self.rpc_max_clients);
        let grpc_adaptor = Adaptor::server(
            self.net_address,
            self.config.bps().after(),
            manager,
            self.core_service.clone(),
            self.core_service.notifier(),
            self.core_service.subscription_context(),
            self.broadcasters,
            self.counters.clone(),
        );

        // Signal the server was started
        self.started.trigger.trigger();

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
                    warn!("{} error while stopping the connection handler: {}", Self::IDENT, err);
                }
            }

            // On exit, the adaptor is dropped, causing the server termination
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", Self::IDENT);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", Self::IDENT);
            Ok(())
        })
    }
}
