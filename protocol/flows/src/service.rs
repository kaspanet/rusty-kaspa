use std::sync::Arc;

use kaspa_addressmanager::NetAddress;
use kaspa_connectionmanager::{AllowedNetworks, ConnectionManager};
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_p2p_lib::{Adaptor, SocksProxyConfig};
use kaspa_utils::triggers::SingleTrigger;
use kaspa_utils_tower::counters::TowerConnectionCounters;

use crate::flow_context::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    flow_context: Arc<FlowContext>,
    connect_peers: Vec<NetAddress>,
    add_peers: Vec<NetAddress>,
    listen: NetAddress,
    outbound_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    shutdown: SingleTrigger,
    counters: Arc<TowerConnectionCounters>,
    allowed_networks: AllowedNetworks,
}

impl P2pService {
    pub fn new(
        flow_context: Arc<FlowContext>,
        connect_peers: Vec<NetAddress>,
        add_peers: Vec<NetAddress>,
        listen: NetAddress,
        outbound_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        counters: Arc<TowerConnectionCounters>,
        allowed_networks: AllowedNetworks,
    ) -> Self {
        Self {
            flow_context,
            connect_peers,
            add_peers,
            shutdown: SingleTrigger::default(),
            listen,
            outbound_target,
            inbound_limit,
            dns_seeders,
            default_port,
            counters,
            allowed_networks,
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

        let default_proxy = self.flow_context.proxy();
        let ipv4_proxy = self.flow_context.proxy_ipv4();
        let ipv6_proxy = self.flow_context.proxy_ipv6();
        let tor_proxy = self.flow_context.tor_proxy();
        let proxy_config = SocksProxyConfig { default: default_proxy, ipv4: ipv4_proxy, ipv6: ipv6_proxy, onion: tor_proxy };
        let socks_proxy = if proxy_config.is_empty() { None } else { Some(proxy_config) };

        let p2p_adaptor = if self.inbound_limit == 0 {
            Adaptor::client_only(self.flow_context.hub().clone(), self.flow_context.clone(), self.counters.clone(), socks_proxy)
        } else {
            Adaptor::bidirectional(
                self.listen,
                self.flow_context.hub().clone(),
                self.flow_context.clone(),
                self.counters.clone(),
                socks_proxy,
            )
            .unwrap()
        };
        let connection_manager = ConnectionManager::new(
            p2p_adaptor.clone(),
            self.outbound_target,
            self.inbound_limit,
            self.dns_seeders,
            self.default_port,
            self.flow_context.address_manager.clone(),
            self.allowed_networks,
        );

        self.flow_context.set_connection_manager(connection_manager.clone());
        self.flow_context.start_async_services();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            if let Some(mut bootstrap_rx) = self.flow_context.tor_bootstrap_receiver() {
                if !*bootstrap_rx.borrow() {
                    info!("P2P service waiting for Tor bootstrap to complete before enabling networking");
                }
                while !*bootstrap_rx.borrow() {
                    if bootstrap_rx.changed().await.is_err() {
                        warn!("Tor bootstrap signal dropped before completion; continuing with P2P startup");
                        break;
                    }
                }
                if *bootstrap_rx.borrow() {
                    trace!("Tor bootstrap complete; starting P2P networking");
                }
            }
            for peer_address in self.connect_peers.iter().cloned().chain(self.add_peers.iter().cloned()) {
                connection_manager.add_connection_request(peer_address, true).await;
            }

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
