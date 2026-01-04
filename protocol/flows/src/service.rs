use std::sync::Arc;

use kaspa_addressmanager::NetAddress;
use kaspa_connectionmanager::ConnectionManager;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_lib::Adaptor;
use kaspa_utils::triggers::SingleTrigger;
use kaspa_utils_tower::counters::TowerConnectionCounters;

use crate::flow_context::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    flow_context: Arc<FlowContext>,
    connect_peers: Vec<NetAddress>,
    add_peers: Vec<NetAddress>,
    listen: NetAddress,
    random_graph_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    shutdown: SingleTrigger,
    counters: Arc<TowerConnectionCounters>,
}

impl P2pService {
    pub fn new(
        flow_context: Arc<FlowContext>,
        connect_peers: Vec<NetAddress>,
        add_peers: Vec<NetAddress>,
        listen: NetAddress,
        random_graph_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        Self {
            flow_context,
            connect_peers,
            add_peers,
            shutdown: SingleTrigger::default(),
            listen,
            random_graph_target,
            inbound_limit,
            dns_seeders,
            default_port,
            counters,
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

        let p2p_adaptor = if self.inbound_limit == 0 {
            Adaptor::client_only(self.flow_context.hub().clone(), self.flow_context.clone(), self.counters.clone())
        } else {
            Adaptor::bidirectional(self.listen, self.flow_context.hub().clone(), self.flow_context.clone(), self.counters.clone())
                .unwrap()
        };

        let connection_manager = ConnectionManager::new(
            p2p_adaptor.clone(),
            self.random_graph_target,
            self.flow_context.perigee_manager.clone(),
            self.inbound_limit,
            self.dns_seeders,
            self.default_port,
            self.flow_context.address_manager.clone(),
        );

        self.flow_context.set_connection_manager(connection_manager.clone());
        self.flow_context.start_async_services();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            connection_manager
                .clone()
                .add_connection_requests(
                    self.connect_peers
                        .iter()
                        .cloned()
                        .chain(self.add_peers.iter().cloned())
                        .map(|addr| (core::net::SocketAddr::new(*addr.ip, addr.port), true))
                        .collect(),
                )
                .await;

            // Keep the P2P server running until a service shutdown signal is received
            shutdown_signal.await;
            // Important for cleanup of the P2P adaptor since we have a reference cycle:
            // flow ctx -> conn manager -> p2p adaptor -> flow ctx (as ConnectionInitializer)
            self.flow_context.drop_connection_manager();
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
        Box::pin(async move {
            trace!("{} stopped", P2P_CORE_SERVICE);
            Ok(())
        })
    }
}
