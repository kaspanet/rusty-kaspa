use self::service::RpcCoreService;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_notify::notifier::ConsensusNotifier;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    core::Core,
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use kaspa_index_core::notifier::IndexNotifier;
use kaspa_mining::manager::MiningManager;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_utils::triggers::DuplexTrigger;
use kaspa_utxoindex::api::DynUtxoIndexApi;
use std::sync::Arc;

pub mod collector;
pub mod converter;
pub mod service;

const RPC_CORE_SERVICE: &str = "rpc-core-service";

/// [`RpcCoreServer`] encapsulates and exposes a [`RpcCoreService`] as an [`AsyncService`].
pub struct RpcCoreServer {
    service: Arc<RpcCoreService>,
    shutdown: DuplexTrigger,
}

impl RpcCoreServer {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        consensus_notifier: Arc<ConsensusNotifier>,
        index_notifier: Option<Arc<IndexNotifier>>,
        mining_manager: Arc<MiningManager>,
        flow_context: Arc<FlowContext>,
        utxoindex: DynUtxoIndexApi,
        config: Arc<Config>,
        core: Arc<Core>,
    ) -> Self {
        let service = Arc::new(RpcCoreService::new(
            consensus_manager,
            consensus_notifier,
            index_notifier,
            mining_manager,
            flow_context,
            utxoindex,
            config,
            core,
        ));
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
