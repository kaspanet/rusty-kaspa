use std::sync::Arc;

use consensus_core::api::DynConsensus;
use kaspa_core::{task::service::AsyncService, trace};
use kaspa_utils::triggers::DuplexTrigger;
use tokio::task::JoinHandle;

use self::{collector::ConsensusNotificationReceiver, service::RpcCoreService};

pub mod collector;
pub mod service;

const RPC_CORE_SERVICE: &str = "rpc-core-service";

pub struct RpcCoreServer {
    service: Arc<RpcCoreService>,
    shutdown: DuplexTrigger,
}

impl RpcCoreServer {
    pub fn new(consensus: DynConsensus, consensus_recv: ConsensusNotificationReceiver) -> Self {
        let service = Arc::new(RpcCoreService::new(consensus, consensus_recv));
        Self { service, shutdown: DuplexTrigger::default() }
    }

    #[inline(always)]
    pub fn service(&self) -> Arc<RpcCoreService> {
        self.service.clone()
    }
}

// This could be opted out in the wasm32 context

impl AsyncService for RpcCoreServer {
    fn ident(self: Arc<Self>) -> &'static str {
        RPC_CORE_SERVICE
    }

    fn start(self: Arc<Self>) -> JoinHandle<()> {
        trace!("{} starting", RPC_CORE_SERVICE);
        let service = self.service.clone();

        // Prepare a shutdown future
        let shutdown_signal = self.shutdown.request.listener.clone();

        // Launch the service and wait for a shutdown signal
        let shutdown_executed = self.shutdown.response.trigger.clone();
        tokio::spawn(async move {
            service.start();
            shutdown_signal.await;
            shutdown_executed.trigger();
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", RPC_CORE_SERVICE);
        self.shutdown.request.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> JoinHandle<()> {
        trace!("{} stopping", RPC_CORE_SERVICE);
        let service = self.service.clone();
        let shutdown_executed_signal = self.shutdown.response.listener.clone();
        tokio::spawn(async move {
            // Wait for the service start task to exit
            shutdown_executed_signal.await;

            // Stop the service
            match service.stop().await {
                Ok(_) => {}
                Err(err) => {
                    trace!("Error while stopping {}: {}", RPC_CORE_SERVICE, err);
                }
            }
            trace!("{} exiting", RPC_CORE_SERVICE);
        })
    }
}
