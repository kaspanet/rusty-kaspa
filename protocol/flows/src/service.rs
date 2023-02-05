use consensus_core::api::DynConsensus;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_utils::triggers::SingleTrigger;
use p2p_lib::adaptor::{P2pAdaptor, P2pAdaptorApi};
use std::sync::Arc;

use crate::ctx::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    consensus: DynConsensus,
    shutdown: SingleTrigger,
}

impl P2pService {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { consensus, shutdown: SingleTrigger::default() }
    }
}

impl AsyncService for P2pService {
    fn ident(self: Arc<Self>) -> &'static str {
        P2P_CORE_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", P2P_CORE_SERVICE);

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            let ip_port = String::from("[::1]:50051");
            let ctx = Arc::new(FlowContext::new(self.consensus.clone()));
            let p2p_adaptor = P2pAdaptor::listen(ip_port.clone(), ctx).await.unwrap();

            // For now, attempt to connect to a running golang node
            let golang_ip_port = String::from("http://[::1]:16111");
            trace!("P2P, p2p::main - starting peer:{golang_ip_port}");
            let _peer_id = p2p_adaptor.connect_peer(golang_ip_port.clone()).await;

            // Keep the P2P server running until an app shutdown signal is triggered
            shutdown_signal.await;
            p2p_adaptor.terminate_all_peers_and_flows().await;
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", P2P_CORE_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", P2P_CORE_SERVICE);
        Box::pin(async move {
            trace!("{} exiting", P2P_CORE_SERVICE);
        })
    }
}
