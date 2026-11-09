use std::sync::Arc;

use kaspa_addressmanager::{LocalAdvertisementError, NetAddress};
use kaspa_connectionmanager::ConnectionManager;
use kaspa_consensus_core::network::NetworkType;
use kaspa_core::{
    core::Core,
    error,
    signals::Shutdown,
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_lib::{
    Adaptor, ConnectionError,
    libp2p_transport::{Libp2pTransport, TransportError},
};
use kaspa_utils::{
    connection::{Libp2pMetadata, PeerAddress},
    triggers::SingleTrigger,
};
use kaspa_utils_tower::counters::TowerConnectionCounters;

use crate::flow_context::FlowContext;

const P2P_CORE_SERVICE: &str = "p2p-service";

pub struct P2pService {
    core: Arc<Core>,
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
}

impl P2pService {
    pub fn new(
        core: Arc<Core>,
        flow_context: Arc<FlowContext>,
        connect_peers: Vec<NetAddress>,
        add_peers: Vec<NetAddress>,
        listen: NetAddress,
        outbound_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        Self {
            core,
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
        }
    }

    /// Publishes the first signed address in memory, then updates it only when the home relay changes.
    fn update_local_advertisement(&self, transport: &Libp2pTransport) -> Result<(), LocalAdvertisementError<TransportError>> {
        let ctx = &self.flow_context;
        let mut manager = ctx.address_manager.lock();
        let network = ctx.config.network_name();
        let Some(endpoint) = transport.endpoint_address() else {
            return Ok(());
        };
        if manager.local_advertisement_initialized() {
            return manager.update_local_relay(endpoint.relay, |address| transport.sign_address(address, &network));
        }
        let tcp: Option<NetAddress> = manager.best_local_address();
        let Some(direct_address) = transport.direct_address() else {
            return Ok(());
        };
        // A loopback libp2p candidate must not displace a known non-loopback
        // advertised listener. The libp2p and Kaspa TCP ports use the same advertised IP.
        let ip = if let Some(external) = ctx.config.externalip {
            external.ip.into()
        } else if direct_address.ip().is_loopback() {
            tcp.filter(|tcp| !tcp.ip.is_loopback()).map_or(direct_address.ip(), |tcp| tcp.ip.into())
        } else {
            direct_address.ip()
        };
        let tcp_port = tcp
            .filter(|tcp| tcp.ip.0.to_canonical() == ip.to_canonical())
            .map_or_else(|| ctx.config.p2p_listen_address.normalize(ctx.config.default_p2p_port()).port, |tcp| tcp.port);
        let address = PeerAddress {
            ip: ip.into(),
            tcp_port,
            libp2p: Some(Libp2pMetadata {
                relay_port: ctx.config.relay_port,
                endpoint_id: endpoint.id,
                port: direct_address.port(),
                nonce: 0,
                signature: Vec::new(),
            }),
            relay_address: endpoint.relay,
        };
        manager.sign_local_address(address, |address| transport.sign_address(address, &network))?;
        Ok(())
    }
}

impl AsyncService for P2pService {
    fn ident(self: Arc<Self>) -> &'static str {
        P2P_CORE_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", P2P_CORE_SERVICE);

        Box::pin(async move {
            let startup = async {
                let allow_private = matches!(self.flow_context.config.net.network_type, NetworkType::Simnet | NetworkType::Devnet);
                let transport = Arc::new(Libp2pTransport::new(self.flow_context.config.relay_port, allow_private)?);
                self.flow_context.address_manager.lock().set_local_libp2p_id(transport.id());

                let p2p_adaptor = if self.inbound_limit == 0 {
                    Adaptor::client_only(self.flow_context.hub().clone(), self.flow_context.clone(), self.counters.clone(), transport)
                } else {
                    Adaptor::bidirectional(
                        self.listen,
                        self.flow_context.hub().clone(),
                        self.flow_context.clone(),
                        self.counters.clone(),
                        transport,
                    )?
                };
                if let Err(error) = p2p_adaptor.start_libp2p(self.listen.ip) {
                    p2p_adaptor.close().await;
                    return Err(ConnectionError::from(error));
                }
                Ok::<_, ConnectionError>(p2p_adaptor)
            }
            .await;
            let p2p_adaptor = startup.map_err(|error| {
                error!("P2P startup failed: {error}");
                self.core.exit_code.store(1, std::sync::atomic::Ordering::Relaxed);
                self.core.shutdown();
                AsyncServiceError::Service(format!("P2P startup failed: {error}"))
            })?;
            let transport = p2p_adaptor.libp2p_transport();
            let connection_manager = ConnectionManager::new(
                p2p_adaptor.clone(),
                self.outbound_target,
                self.inbound_limit,
                self.dns_seeders,
                self.default_port,
                self.flow_context.address_manager.clone(),
            );
            self.flow_context.set_connection_manager(connection_manager.clone());
            self.flow_context.start_async_services();

            for peer_address in self.connect_peers.iter().cloned().chain(self.add_peers.iter().cloned()) {
                connection_manager.add_connection_request(peer_address.into(), true).await;
            }

            // Publish the first signed address, then replace the in-memory record when the home relay changes.
            // TCP bootstrap remains available while libp2p has no usable local candidate.
            let result = loop {
                if let Err(error) = self.update_local_advertisement(&transport) {
                    break Err(format!("local address publication failed: {error}"));
                }
                tokio::select! {
                    _ = self.shutdown.listener.clone() => break Ok(()),
                    _ = transport.address_changed() => {},
                    _ = transport.cancelled() => {
                        let error = transport.take_failure()
                            .unwrap_or(TransportError::Stopped);
                        break Err(format!("P2P transport failed: {error}"));
                    }
                }
            };
            if let Err(error) = &result {
                error!("{error}");
                self.core.exit_code.store(1, std::sync::atomic::Ordering::Relaxed);
                self.core.shutdown();
            }
            // Important for cleanup of the P2P adaptor since we have a reference cycle:
            // flow ctx -> conn manager -> p2p adaptor -> flow ctx (as ConnectionInitializer)
            self.flow_context.drop_connection_manager();
            connection_manager.stop().await;
            p2p_adaptor.close().await;
            result.map_err(AsyncServiceError::Service)
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
