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
    connect: Option<String>, // TEMP: optional connect peer
    listen: Option<String>,
    outbound_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    shutdown: SingleTrigger,
}

impl P2pService {
    pub fn new(
        flow_context: Arc<FlowContext>,
        connect: Option<String>,
        listen: Option<String>,
        outbound_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
    ) -> Self {
        Self {
            flow_context,
            connect,
            shutdown: SingleTrigger::default(),
            listen,
            outbound_target,
            inbound_limit,
            dns_seeders,
            default_port,
        }
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
            let p2p_adaptor =
                Adaptor::bidirectional(server_address.clone(), self.flow_context.hub().clone(), self.flow_context.clone()).unwrap();
            let connection_manager = ConnectionManager::new(
                p2p_adaptor.clone(),
                self.outbound_target,
                self.inbound_limit,
                self.dns_seeders,
                self.default_port,
                self.flow_context.amgr.clone(),
            );

            // For now, attempt to connect to a running golang node
            if let Some(peer_address) = self.connect.clone() {
                connection_manager.add_connection_request(peer_address.to_socket_addrs().unwrap().next().unwrap(), true).await;
            }

            // Keep the P2P server running until a service shutdown signal is received
            shutdown_signal.await;
            p2p_adaptor.terminate_all_peers().await;
            connection_manager.stop().await;
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
