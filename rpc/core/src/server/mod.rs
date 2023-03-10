use consensus_core::api::DynConsensus;
use consensus_notify::service::NotifyService;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use kaspa_index_processor::service::IndexService;
use kaspa_utils::triggers::DuplexTrigger;
use std::sync::Arc;

use self::service::RpcCoreService;

pub mod collector;
pub mod service;

const RPC_CORE_SERVICE: &str = "rpc-core-service";

/// [`RpcCoreServer`] encapsulates and exposes a [`RpcCoreService`] as an [`AsyncService`].
pub struct RpcCoreServer {
    service: Arc<RpcCoreService>,
    shutdown: DuplexTrigger,
}

impl RpcCoreServer {
    pub fn new(consensus: DynConsensus, notify_service: Arc<NotifyService>, index_service: Option<Arc<IndexService>>) -> Self {
        let service = Arc::new(RpcCoreService::new(consensus, notify_service, index_service));
        Self { service, shutdown: DuplexTrigger::default() }
    }

    #[inline(always)]
    pub fn service(&self) -> Arc<RpcCoreService> {
        self.service.clone()
    }
}

// It might be necessary to opt this out in the context of wasm32

impl AsyncService for RpcCoreServer {
    fn ident(self: Arc<Self>) -> &'static str {
        RPC_CORE_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", RPC_CORE_SERVICE);
        let service = self.service.clone();

        // Prepare a start shutdown signal receiver and a shutdown ended signal sender
        let shutdown_signal = self.shutdown.request.listener.clone();
        let shutdown_executed = self.shutdown.response.trigger.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // TODO - check error handling
            service.start();
            shutdown_signal.await;
            shutdown_executed.trigger();
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", RPC_CORE_SERVICE);
        self.shutdown.request.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", RPC_CORE_SERVICE);
        let service = self.service.clone();
        let shutdown_executed_signal = self.shutdown.response.listener.clone();
        Box::pin(async move {
            // Wait for the service start task to exit
            shutdown_executed_signal.await;

            trace!("{} exiting", RPC_CORE_SERVICE);
            // Stop the service
            service.stop().await.map_err(|err| AsyncServiceError::Service(format!("Error while stopping {RPC_CORE_SERVICE}: `{err}`")))
        })
    }
}
