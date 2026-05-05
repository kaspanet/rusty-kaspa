use std::{sync::Arc, time::Duration};

use kaspa_addressmanager::NetAddress;
use kaspa_connectionmanager::{ConnectionManager, HostnameResolver, TokioHostnameResolver};
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_lib::Adaptor;
use kaspa_utils::{networking::PeerEndpoint, triggers::SingleTrigger};
use kaspa_utils_tower::counters::TowerConnectionCounters;

use crate::flow_context::FlowContext;

/// Service identifier registered with the async runtime; exposed so
/// integration harnesses can downcast the running [`P2pService`] from
/// the running daemon.
pub const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    flow_context: Arc<FlowContext>,
    connect_peers: Vec<PeerEndpoint>,
    add_peers: Vec<PeerEndpoint>,
    listen: NetAddress,
    outbound_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    /// Cadence for periodic hostname re-resolution. `Duration::ZERO`
    /// disables periodic refresh; dial-failure-triggered re-resolution
    /// remains active in either case.
    hostname_refresh_interval: Duration,
    /// Async DNS resolver dependency. Production wires
    /// [`TokioHostnameResolver`]; tests can supply a fake.
    resolver: Arc<dyn HostnameResolver>,
    shutdown: SingleTrigger,
    counters: Arc<TowerConnectionCounters>,
}

impl P2pService {
    pub fn new(
        flow_context: Arc<FlowContext>,
        connect_peers: Vec<PeerEndpoint>,
        add_peers: Vec<PeerEndpoint>,
        listen: NetAddress,
        outbound_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        hostname_refresh_interval: Duration,
        counters: Arc<TowerConnectionCounters>,
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
            hostname_refresh_interval,
            resolver: Arc::new(TokioHostnameResolver),
            counters,
        }
    }

    /// Replace the default Tokio resolver. Test seam.
    pub fn with_resolver(mut self, resolver: Arc<dyn HostnameResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Test-support accessor: yields the [`FlowContext`] so integration
    /// harnesses can navigate to the [`kaspa_connectionmanager::ConnectionManager`]
    /// for read-only metric scraping during behavior assertions.
    pub fn flow_context(&self) -> Arc<crate::flow_context::FlowContext> {
        self.flow_context.clone()
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
            self.outbound_target,
            self.inbound_limit,
            self.dns_seeders,
            self.default_port,
            self.flow_context.address_manager.clone(),
            self.hostname_refresh_interval,
            self.resolver.clone(),
        );

        self.flow_context.set_connection_manager(connection_manager.clone());
        self.flow_context.start_async_services();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            // Register every operator-pinned endpoint with the connection
            // manager. A hostname that does not currently resolve is
            // registered for periodic retry rather than aborting startup;
            // the unresolvable-host path and the unreachable-IP path share
            // the same retry-forever loop. Warn-level logging and metric
            // accounting for the resolution-failure path live in the
            // connection manager.
            //
            // Registrations run concurrently via `join_all` so the worst-
            // case startup wall-clock is bounded by the slowest single
            // resolve (the resolver enforces its own `~5s` per-host
            // timeout) rather than `N x timeout`. `add_endpoint_request`
            // is independent per host -- the connection manager's
            // internal locking serializes the registry mutations -- so
            // concurrent calls are safe.
            //
            // Source: https://github.com/bitcoin/bitcoin/blob/8f4a3ba8972dae9412ba975a040cea22c227f983/src/net.cpp#L2974
            // (`CConnman::ThreadOpenAddedConnections`).
            let endpoints: Vec<PeerEndpoint> = self.connect_peers.iter().cloned().chain(self.add_peers.iter().cloned()).collect();
            futures::future::join_all(
                endpoints.into_iter().map(|endpoint| connection_manager.add_endpoint_request(endpoint, true, self.default_port)),
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
