use consensus_core::api::DynConsensus;
use kaspa_core::{
    debug,
    task::service::{AsyncService, AsyncServiceFuture},
};
use p2p_lib::adaptor::{P2pAdaptor, P2pAdaptorApi};
use std::sync::Arc;

use crate::ctx::FlowContext;

pub struct P2pService {
    consensus: DynConsensus,
}

impl P2pService {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { consensus }
    }
}

impl AsyncService for P2pService {
    fn ident(self: Arc<Self>) -> &'static str {
        "p2p"
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        // trace!("{} starting", RPC_CORE_SERVICE);
        // let service = self.service.clone();

        // // Prepare a start shutdown signal receiver and a shutdown ended signal sender
        // let shutdown_signal = self.shutdown.request.listener.clone();
        // let shutdown_executed = self.shutdown.response.trigger.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // service.start();
            // shutdown_signal.await;
            // shutdown_executed.trigger();

            let ip_port = String::from("[::1]:50051");
            let ctx = Arc::new(FlowContext::new(self.consensus.clone()));
            let p2p_adaptor = P2pAdaptor::listen(ip_port.clone(), ctx).await.unwrap();

            let other_ip_port = String::from("http://[::1]:16111");
            debug!("P2P, p2p::main - starting peer:{other_ip_port}");
            let _peer_id = p2p_adaptor.connect_peer(other_ip_port.clone()).await;

            tokio::time::sleep(std::time::Duration::from_secs(32)).await;
        })
    }

    fn signal_exit(self: Arc<Self>) {
        // trace!("sending an exit signal to {}", RPC_CORE_SERVICE);
        // self.shutdown.request.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        // trace!("{} stopping", RPC_CORE_SERVICE);
        // let service = self.service.clone();
        // let shutdown_executed_signal = self.shutdown.response.listener.clone();
        Box::pin(async move {
            // // Wait for the service start task to exit
            // shutdown_executed_signal.await;

            // // Stop the service
            // match service.stop().await {
            //     Ok(_) => {}
            //     Err(err) => {
            //         trace!("Error while stopping {}: {}", RPC_CORE_SERVICE, err);
            //     }
            // }
            // trace!("{} exiting", RPC_CORE_SERVICE);
        })
    }
}
