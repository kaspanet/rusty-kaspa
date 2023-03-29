use kaspa_connectionmanager::ConnectionManager;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_lib::Adaptor;
use kaspa_utils::triggers::SingleTrigger;
use std::{net::ToSocketAddrs, sync::Arc};

use crate::flow_context::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    flow_context: Arc<FlowContext>,
    p2p_adaptor: Arc<Adaptor>,
    connect: Option<String>, // TEMP: optional connect peer
    outbound_target: usize,
    inbound_limit: usize,
    shutdown: SingleTrigger,
}

impl P2pService {
    pub fn new(
        flow_context: Arc<FlowContext>,
        p2p_adaptor: Arc<Adaptor>,
        connect: Option<String>,
        outbound_target: usize,
        inbound_limit: usize,
    ) -> Self {
        Self { flow_context, p2p_adaptor, connect, shutdown: SingleTrigger::default(), outbound_target, inbound_limit }
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
            let connection_manager = ConnectionManager::new(
                self.p2p_adaptor.clone(),
                self.outbound_target,
                self.inbound_limit,
                self.flow_context.amgr.clone(),
            );

            // For now, attempt to connect to a running golang node
            if let Some(peer_address) = self.connect.clone() {
                connection_manager.add_connection_request(peer_address.to_socket_addrs().unwrap().next().unwrap(), true).await;
            }

            // Keep the P2P server running until a service shutdown signal is received
            shutdown_signal.await;
            self.p2p_adaptor.terminate_all_peers().await;
            connection_manager.stop().await;
            // TODO: close hub_sender
            Ok(())
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
            Ok(())
        })
    }
}
