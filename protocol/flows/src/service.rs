use consensus_core::api::DynConsensus;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_utils::triggers::SingleTrigger;
use p2p_lib::Adaptor;
use std::{sync::Arc, time::Duration};

use crate::flow_context::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    consensus: DynConsensus,
    connect: Option<String>, // TEMP: optional connect peer
    listen: Option<String>,
    shutdown: SingleTrigger,
}

impl P2pService {
    pub fn new(consensus: DynConsensus, connect: Option<String>, listen: Option<String>) -> Self {
        Self { consensus, connect, shutdown: SingleTrigger::default(), listen }
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
            let server_address = self.listen.clone().unwrap_or(String::from("[::1]:50051"));
            let ctx = Arc::new(FlowContext::new(self.consensus.clone()));
            let p2p_adaptor = Adaptor::bidirectional(server_address.clone(), ctx).unwrap();

            // For now, attempt to connect to a running golang node
            if let Some(peer_address) = self.connect.clone() {
                trace!("P2P, p2p::main - starting peer:{peer_address}");
                let _peer_id = p2p_adaptor.connect_peer_with_retry_params(peer_address, 1, Duration::from_secs(1)).await;
            }

            // Keep the P2P server running until a service shutdown signal is received
            shutdown_signal.await;
            p2p_adaptor.terminate_all_peers().await;
            // drop(p2p_adaptor);
            // tokio::time::sleep(std::time::Duration::from_secs(1)).await;
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
