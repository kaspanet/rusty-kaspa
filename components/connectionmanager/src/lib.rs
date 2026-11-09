use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::{Duration, SystemTime},
};

use duration_string::DurationString;
use futures_util::future::{join_all, try_join_all};
use itertools::Itertools;
use kaspa_addressmanager::{AddressManager, NetAddress};
use kaspa_core::{debug, info, warn};
use kaspa_p2p_lib::{ConnectionError, Peer, PeerKey, common::ProtocolError};
use kaspa_utils::{
    connection::{LibP2pAddress, PeerAddress},
    triggers::SingleTrigger,
};
use parking_lot::Mutex as ParkingLotMutex;
use rand::{seq::SliceRandom, thread_rng};
use tokio::{
    select,
    sync::{
        Mutex as TokioMutex, Semaphore,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
    time::{MissedTickBehavior, interval},
};

pub struct ConnectionManager {
    p2p_adaptor: Arc<kaspa_p2p_lib::Adaptor>,
    outbound_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    address_manager: Arc<ParkingLotMutex<AddressManager>>,
    connection_requests: TokioMutex<HashMap<SocketAddr, ConnectionRequest>>,
    force_next_iteration: UnboundedSender<()>,
    shutdown_signal: SingleTrigger,
    tasks: tokio_util::task::TaskTracker,
}

#[derive(Clone, Debug)]
struct ConnectionRequest {
    next_attempt: SystemTime,
    is_permanent: bool,
    attempts: u32,
}

impl ConnectionRequest {
    fn new(is_permanent: bool) -> Self {
        Self { next_attempt: SystemTime::now(), is_permanent, attempts: 0 }
    }
}

impl ConnectionManager {
    pub fn new(
        p2p_adaptor: Arc<kaspa_p2p_lib::Adaptor>,
        outbound_target: usize,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        address_manager: Arc<ParkingLotMutex<AddressManager>>,
    ) -> Arc<Self> {
        let (tx, rx) = unbounded_channel::<()>();
        let manager = Arc::new(Self {
            p2p_adaptor,
            outbound_target,
            inbound_limit,
            address_manager,
            connection_requests: Default::default(),
            force_next_iteration: tx,
            shutdown_signal: SingleTrigger::new(),
            tasks: tokio_util::task::TaskTracker::new(),
            dns_seeders,
            default_port,
        });
        manager.clone().start_event_loop(rx);
        manager.clone().start_libp2p_event_loop();
        // Fire-and-forget wakeup: if the event loop is already gone the wakeup is moot, so ignore
        // a send error rather than panicking (the receiver is dropped on shutdown).
        let _ = manager.force_next_iteration.send(());
        manager
    }

    /// Maintains this node's home relay: keeps an active reservation, records
    /// failures, and tries known relay candidates when a replacement is needed.
    fn start_libp2p_event_loop(self: Arc<Self>) {
        let transport = self.p2p_adaptor.libp2p_transport();
        let task_transport = transport.clone();
        let tasks = self.tasks.clone();
        tasks.spawn(async move {
            let mut ticker = interval(Duration::from_secs(5));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut selected: Option<PeerAddress> = None;
            loop {
                select! {
                    _ = task_transport.cancelled() => break,
                    _ = self.shutdown_signal.listener.clone() => break,
                    _ = ticker.tick() => {
                        let work = async {
                            if let Some(record) = &selected {
                                let address = SocketAddr::new(record.ip.into(), record.relay_port().unwrap());
                                let relay = LibP2pAddress { address, id: record.endpoint_id().unwrap() };
                                let unavailable = {
                                    let mut manager = self.address_manager.lock();
                                    // We switch relay if the current one is banned, removed from the address manager, or switched the relay port.
                                    manager.is_banned(address.ip().into()) || manager.address(record.key()).is_none_or(|current| current.relay_port() != record.relay_port())
                                };
                                if unavailable {
                                    task_transport.remove_relay(relay).await;
                                    selected = None;
                                } else if !task_transport.has_relay_reservation(relay) {
                                    warn!("libp2p selected relay failed: {address}");
                                    self.address_manager.lock().record_relay_result(record, false);
                                    task_transport.remove_relay(relay).await;
                                    selected = None;
                                }
                            }
                            if selected.is_none() {
                                let candidates = self.address_manager.lock().iterate_prioritized_random_relays();
                                for (record, failures) in candidates {
                                    let address = SocketAddr::new(record.ip.into(), record.relay_port().unwrap());
                                    let relay = LibP2pAddress { address, id: record.endpoint_id().unwrap() };
                                    info!("libp2p selecting relay {address}, failed attempts: {failures}");
                                    match task_transport.register_relay(relay).await {
                                        Ok(()) => {
                                            self.address_manager.lock().record_relay_result(&record, true);
                                            selected = Some(record);
                                            break;
                                        }
                                        Err(error @ ConnectionError::AtCapacity) => {
                                            debug!("libp2p relay registration deferred for {address}: {error}");
                                            break;
                                        }
                                        Err(error) => {
                                            warn!("libp2p relay registration failed for {address}: {error}");
                                            self.address_manager.lock().record_relay_result(&record, false);
                                        }
                                    }
                                }
                            }

                        };
                        select! {
                            _ = task_transport.cancelled() => break,
                            _ = self.shutdown_signal.listener.clone() => break,
                            _ = work => {}
                        }
                    }
                }
            }
        });
    }

    fn start_event_loop(self: Arc<Self>, mut rx: UnboundedReceiver<()>) {
        let mut ticker = interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let tasks = self.tasks.clone();
        tasks.spawn(async move {
            loop {
                if self.shutdown_signal.trigger.is_triggered() {
                    break;
                }
                select! {
                    _ = rx.recv() => {},
                    _ = ticker.tick() => {},
                    _ = self.shutdown_signal.listener.clone() => break,
                }
                select! {
                    _ = self.shutdown_signal.listener.clone() => break,
                    _ = self.clone().handle_event() => {}
                }
            }
            debug!("Connection manager event loop exiting");
        });
    }

    async fn handle_event(self: Arc<Self>) {
        debug!("Starting connection loop iteration");
        let peer_by_key: HashMap<PeerKey, Peer> = self.p2p_adaptor.active_peers().into_iter().map(|peer| (peer.key(), peer)).collect();

        self.handle_connection_requests(&peer_by_key).await;
        let peer_by_key = self.p2p_adaptor.active_peers().into_iter().map(|peer| (peer.key(), peer)).collect();
        self.handle_outbound_connections(&peer_by_key).await;
        self.handle_inbound_connections(&peer_by_key).await;
    }

    pub fn has_inbound_capacity(&self) -> bool {
        self.p2p_adaptor.peers_query(false) < self.inbound_limit
    }

    pub async fn add_connection_request(&self, address: SocketAddr, is_permanent: bool) {
        // If the request already exists, it resets the attempts count and overrides the `is_permanent` setting.
        self.connection_requests.lock().await.insert(address, ConnectionRequest::new(is_permanent));
        // Force the next iteration of the connection loop. This is a fire-and-forget wakeup, so if
        // the event loop receiver was already dropped (e.g. during shutdown) the send is a no-op
        // rather than a panic. add_connection_request is reachable from the AddPeer RPC handler,
        // which can race with shutdown.
        let _ = self.force_next_iteration.send(());
    }

    pub async fn stop(&self) {
        self.shutdown_signal.trigger.trigger();
        self.tasks.close();
        self.tasks.wait().await;
    }

    async fn handle_connection_requests(self: &Arc<Self>, peer_by_key: &HashMap<PeerKey, Peer>) {
        let connected_tcp: HashSet<SocketAddr> =
            peer_by_key.values().filter(|peer| peer.libp2p_id().is_none()).map(Peer::net_address).collect();
        let mut requests = self.connection_requests.lock().await;
        let mut new_requests = HashMap::with_capacity(requests.len());
        for (address, request) in requests.iter() {
            let address = *address;
            let request = request.clone();
            let is_connected = connected_tcp.contains(&address);
            if is_connected && !request.is_permanent {
                // The peer is connected and the request is not permanent - no need to keep the request
                continue;
            }

            if !is_connected && request.next_attempt <= SystemTime::now() {
                debug!("Connecting to peer request {}", address);
                match self.p2p_adaptor.connect_peer(address.to_string()).await {
                    Err(err) => {
                        debug!("Failed connecting to peer request: {}, {}", address, err);
                        if request.is_permanent {
                            const MAX_ACCOUNTABLE_ATTEMPTS: u32 = 4;

                            #[allow(clippy::arithmetic_side_effects, reason = "`retry_duration <= 30 * 2^4 = 480 seconds`.")]
                            let retry_duration =
                                Duration::from_secs(30u64 * 2u64.pow(min(request.attempts, MAX_ACCOUNTABLE_ATTEMPTS)));
                            debug!("Will retry peer request {} in {}", address, DurationString::from(retry_duration));
                            #[allow(clippy::arithmetic_side_effects, reason = "`retry_duration` is small enough.")]
                            let next_attempt = SystemTime::now() + retry_duration;
                            let attempts = request.attempts.saturating_add(1);
                            new_requests.insert(address, ConnectionRequest { next_attempt, attempts, is_permanent: true });
                        }
                    }
                    Ok(_) if request.is_permanent => {
                        // Permanent requests are kept forever
                        new_requests.insert(address, ConnectionRequest::new(true));
                    }
                    Ok(_) => {}
                }
            } else {
                new_requests.insert(address, request);
            }
        }

        *requests = new_requests;
    }

    async fn handle_outbound_connections(self: &Arc<Self>, peer_by_key: &HashMap<PeerKey, Peer>) {
        let Some(mut missing_connections) =
            self.outbound_target.checked_sub(peer_by_key.values().filter(|peer| peer.is_outbound()).count())
        else {
            return;
        };

        if missing_connections == 0 {
            return;
        }

        let transport = self.p2p_adaptor.libp2p_transport();
        let libp2p_outbound = transport.outbound_sessions();
        // Libp2p connections can bypass our IP-prefix diversification policy.
        // Limit their share to keep most outbound connections subject to this policy.
        let libp2p_slots = Arc::new(Semaphore::new((self.outbound_target / 2).saturating_sub(libp2p_outbound)));
        let mut addr_iter = {
            let manager = self.address_manager.lock();
            let mut tcp_exceptions = HashSet::new();
            let mut libp2p_exceptions = HashSet::new();
            for peer in peer_by_key.values() {
                if let Some(id) = peer.libp2p_id() {
                    libp2p_exceptions.insert(id);
                } else {
                    let socket = peer.net_address();
                    tcp_exceptions.insert(NetAddress::new(socket.ip().to_canonical().into(), socket.port()));
                }
            }
            manager.iterate_prioritized_random_addresses(tcp_exceptions, libp2p_exceptions)
        };
        let mut dialing_ids = HashSet::new();
        let mut dialing_sockets = HashSet::new();
        let mut progressing = true;
        while addr_iter.len() > 0 && missing_connections > 0 {
            if self.shutdown_signal.trigger.is_triggered() {
                return;
            }
            let mut addrs_to_connect = Vec::with_capacity(missing_connections);
            let mut jobs = Vec::with_capacity(missing_connections);
            while jobs.len() < missing_connections {
                let Some(address) = addr_iter.next() else {
                    break;
                };
                if self.address_manager.lock().is_banned(address.ip) {
                    continue;
                }
                let socket = SocketAddr::from(address.tcp_address());
                if dialing_sockets.contains(&socket) {
                    continue;
                }
                if let Some(id) = address.endpoint_id()
                    && (transport.id() == id || !dialing_ids.insert(id))
                {
                    continue;
                }
                dialing_sockets.insert(socket);
                debug!("Connecting to {:?}", address);
                addrs_to_connect.push(address.clone());
                let adaptor = self.p2p_adaptor.clone();
                let libp2p_slots = libp2p_slots.clone();
                jobs.push(async move {
                    let result = adaptor.connect_peer(address.tcp_address().to_string()).await;
                    match result {
                        Ok(_) | Err(ConnectionError::ProtocolError(ProtocolError::PeerAlreadyExists(_))) => result,
                        Err(tcp_error) => match address.endpoint() {
                            Some(endpoint) if endpoint.relay.is_some() && adaptor.libp2p_transport().is_ready() => {
                                let permit = libp2p_slots.try_acquire_owned().map_err(|_| ConnectionError::AtCapacity)?;
                                let result = adaptor.connect_libp2p_peer(&endpoint).await;
                                if result.is_ok() {
                                    // Keep the successful session's slot consumed for the rest of this iteration.
                                    permit.forget();
                                }
                                result
                            }
                            _ => Err(tcp_error),
                        },
                    }
                });
            }
            // The iterator is exhausted and all remaining candidates were skipped, so there is nothing left to dial.
            if jobs.is_empty() {
                break;
            }
            #[allow(clippy::arithmetic_side_effects, reason = "`missing_connections <= self.outbound_target`")]
            let outgoing = self.outbound_target - missing_connections;
            if progressing && !jobs.is_empty() {
                // Log only if progress was made
                info!(
                    "Connection manager: has {}/{} outgoing P2P connections, trying to obtain {} additional connection(s)...",
                    outgoing,
                    self.outbound_target,
                    jobs.len(),
                );
                progressing = false;
            } else {
                debug!(
                    "Connection manager: outgoing: {}/{} , connecting: {}, iterator: {}",
                    outgoing,
                    self.outbound_target,
                    jobs.len(),
                    addr_iter.len(),
                );
            }
            for (res, net_addr) in (join_all(jobs).await).into_iter().zip(addrs_to_connect) {
                match res {
                    Ok(_) => {
                        self.address_manager.lock().mark_connection_success(net_addr);
                        #[allow(clippy::arithmetic_side_effects, reason = "We checked above that `missing_connections > 0`.")]
                        {
                            missing_connections -= 1;
                        }
                        progressing = true;
                    }
                    Err(ConnectionError::ProtocolError(ProtocolError::PeerAlreadyExists(_))) => {
                        // We avoid marking the existing connection as connection failure
                        debug!("Failed connecting to {:?}, peer already exists", net_addr);
                    }
                    Err(error @ ConnectionError::AtCapacity) => {
                        // Local transport capacity is not evidence against the peer.
                        // Retry this record on the next connection-manager iteration.
                        debug!("Deferring {:?}: {}", net_addr, error);
                    }
                    Err(err) => {
                        debug!("Failed connecting to {:?}, err: {}", net_addr, err);
                        self.address_manager.lock().mark_connection_failure(net_addr);
                    }
                }
            }
        }

        if missing_connections > 0 && !self.dns_seeders.is_empty() {
            if missing_connections > self.outbound_target / 2 {
                // If we are missing more than half of our target, query all in parallel.
                // This will always be the case on new node start-up and is the most resilient strategy in such a case.
                self.dns_seed_many(self.dns_seeders.len()).await;
            } else {
                // Try to obtain at least twice the number of missing connections
                #[allow(clippy::arithmetic_side_effects, reason = "`missing_connections <= self.outbound_target / 2`")]
                self.dns_seed_with_address_target(2 * missing_connections).await;
            }
        }
    }

    async fn handle_inbound_connections(self: &Arc<Self>, peer_by_key: &HashMap<PeerKey, Peer>) {
        let active_inbound = peer_by_key.values().filter(|peer| !peer.is_outbound()).collect_vec();
        let active_inbound_len = active_inbound.len();
        let Some(inbounds_to_terminate) = active_inbound_len.checked_sub(self.inbound_limit) else {
            return;
        };
        if inbounds_to_terminate == 0 {
            return;
        }

        let mut futures = Vec::with_capacity(inbounds_to_terminate);
        for peer in active_inbound.choose_multiple(&mut thread_rng(), inbounds_to_terminate) {
            debug!("Disconnecting from {} because we're above the inbound limit", peer.net_address());
            futures.push(self.p2p_adaptor.terminate(peer.key()));
        }
        join_all(futures).await;
    }

    /// Queries DNS seeders in random order, one after the other, until obtaining `min_addresses_to_fetch` addresses
    async fn dns_seed_with_address_target(self: &Arc<Self>, min_addresses_to_fetch: usize) {
        let cmgr = self.clone();
        tokio::task::spawn_blocking(move || cmgr.dns_seed_with_address_target_blocking(min_addresses_to_fetch)).await.unwrap();
    }

    fn dns_seed_with_address_target_blocking(self: &Arc<Self>, mut min_addresses_to_fetch: usize) {
        let shuffled_dns_seeders = self.dns_seeders.choose_multiple(&mut thread_rng(), self.dns_seeders.len());
        for &seeder in shuffled_dns_seeders {
            // Query seeders sequentially until reaching the desired number of addresses
            let addrs_len = self.dns_seed_single(seeder);
            let Some(new_min) = min_addresses_to_fetch.checked_sub(addrs_len) else {
                break;
            };
            if new_min == 0 {
                break;
            }
            min_addresses_to_fetch = new_min;
        }
    }

    /// Queries `num_seeders_to_query` random DNS seeders in parallel
    async fn dns_seed_many(self: &Arc<Self>, num_seeders_to_query: usize) -> usize {
        info!("Querying {} DNS seeders", num_seeders_to_query);
        let shuffled_dns_seeders = self.dns_seeders.choose_multiple(&mut thread_rng(), num_seeders_to_query);
        let jobs = shuffled_dns_seeders.map(|seeder| {
            let cmgr = self.clone();
            tokio::task::spawn_blocking(move || cmgr.dns_seed_single(seeder))
        });
        try_join_all(jobs).await.unwrap().into_iter().sum()
    }

    /// Query a single DNS seeder and add the obtained addresses to the address manager.
    ///
    /// DNS lookup is a blocking i/o operation so this function is assumed to be called
    /// from a blocking execution context.
    fn dns_seed_single(self: &Arc<Self>, seeder: &str) -> usize {
        info!("Querying DNS seeder {}", seeder);
        // Since the DNS lookup protocol doesn't come with a port, we must assume that the default port is used.
        let addrs = match (seeder, self.default_port).to_socket_addrs() {
            Ok(addrs) => addrs,
            Err(e) => {
                warn!("Error connecting to DNS seeder {}: {}", seeder, e);
                return 0;
            }
        };

        let addrs_len = addrs.len();
        info!("Retrieved {} addresses from DNS seeder {}", addrs_len, seeder);
        let mut amgr_lock = self.address_manager.lock();
        for addr in addrs {
            amgr_lock.add_address(NetAddress::new(addr.ip().into(), addr.port()).into());
        }

        addrs_len
    }

    /// Bans the given IP and disconnects from all the peers with that IP.
    ///
    /// _GO-KASPAD: BanByIP_
    pub async fn ban(&self, ip: IpAddr) {
        if self.ip_has_permanent_connection(ip).await {
            return;
        }
        for peer in self.p2p_adaptor.active_peers() {
            if peer.net_address().ip() == ip {
                self.p2p_adaptor.terminate(peer.key()).await;
            }
        }
        self.address_manager.lock().ban(ip.into());
    }

    /// Returns whether the given address is banned.
    pub async fn is_banned(&self, address: &SocketAddr) -> bool {
        !self.is_permanent(address).await && self.address_manager.lock().is_banned(address.ip().into())
    }

    /// Returns whether the given address is a permanent request.
    pub async fn is_permanent(&self, address: &SocketAddr) -> bool {
        self.connection_requests.lock().await.contains_key(address)
    }

    /// Returns whether the given IP has some permanent request.
    pub async fn ip_has_permanent_connection(&self, ip: IpAddr) -> bool {
        self.connection_requests.lock().await.iter().any(|(address, request)| request.is_permanent && address.ip() == ip)
    }
}

#[cfg(test)]
mod libp2p_tests {
    use super::*;
    use kaspa_addressmanager::DbAddressesStore;
    use kaspa_consensus_core::config::{Config, params::SIMNET_PARAMS};
    use kaspa_core::task::tick::TickService;
    use kaspa_database::{
        create_temp_db,
        prelude::{CachePolicy, ConnBuilder, DB},
    };
    use kaspa_p2p_lib::{
        Adaptor, Hub,
        echo::EchoFlowInitializer,
        libp2p_transport::{Libp2pTransport, MAX_PENDING_DIALS, load_identity},
    };
    use kaspa_utils::connection::Libp2pMetadata;

    fn free_port_pair() -> NetAddress {
        loop {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            if port < u16::MAX
                && std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok()
                && std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok()
            {
                return SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)).into();
            }
        }
    }

    async fn node(directory: &std::path::Path, name: &str, tcp: bool) -> (Arc<Adaptor>, NetAddress) {
        let listen = free_port_pair();
        let (key, _) = load_identity(directory.join(name)).await.unwrap();
        let transport = Arc::new(Libp2pTransport::new(key, listen.port + 1, true).unwrap());
        let adaptor = if tcp {
            Adaptor::bidirectional(listen, Hub::new(), Arc::new(EchoFlowInitializer::new()), Default::default(), transport).unwrap()
        } else {
            Adaptor::client_only(Hub::new(), Arc::new(EchoFlowInitializer::new()), Default::default(), transport)
        };
        adaptor.start_libp2p(listen.ip).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while adaptor.libp2p_transport().direct_address().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        (adaptor, listen)
    }

    async fn reserve_relay(adaptor: &Adaptor, relay: &Adaptor) {
        let transport = relay.libp2p_transport();
        let address = SocketAddr::new(transport.direct_address().unwrap().ip(), transport.relay_port());
        adaptor.libp2p_transport().register_relay(LibP2pAddress { address, id: transport.id() }).await.unwrap();
    }

    fn record(adaptor: &Adaptor, tcp: NetAddress) -> PeerAddress {
        let transport = adaptor.libp2p_transport();
        let endpoint = transport.endpoint_address().unwrap();
        let mut record = PeerAddress {
            libp2p: Some(Libp2pMetadata {
                endpoint_id: endpoint.id,
                port: transport.direct_address().unwrap().port(),
                relay_port: transport.relay_port(),
                nonce: 1,
                signature: Vec::new(),
            }),
            relay_address: endpoint.relay,
            ..tcp.into()
        };
        let signature = transport.sign_address(&record, "kaspa-simnet").unwrap();
        record.libp2p.as_mut().unwrap().signature = signature;
        record
    }

    fn address_manager(db: Arc<DB>) -> Arc<ParkingLotMutex<AddressManager>> {
        let mut config = Config::new(SIMNET_PARAMS);
        config.disable_upnp = true;
        AddressManager::new(Arc::new(config), db, Arc::new(TickService::default())).0
    }

    async fn wait(mut condition: impl FnMut() -> bool, seconds: u64) {
        tokio::time::timeout(Duration::from_secs(seconds), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn more_than_32_libp2p_candidates_connect_without_capacity_failure_counts() {
        kaspa_core::log::try_init_logger(
            "info,kaspa_connectionmanager=debug,kaspa_p2p_lib::libp2p_transport=debug,libp2p=warn,noq=warn",
        );
        let directory = tempfile::tempdir().unwrap();
        let (client, _) = node(directory.path(), "client", false).await;
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        let addresses = address_manager(db);
        let mut servers = Vec::new();
        let mut relays = Vec::new();
        for index in 0..MAX_PENDING_DIALS + 8 {
            if index % 4 == 0 {
                relays.push(node(directory.path(), &format!("relay-{index}"), false).await.0);
            }
            let (server, listen) = node(directory.path(), &format!("server-{index}"), false).await;
            reserve_relay(&server, relays.last().unwrap()).await;
            // Distinct endpoint IDs must remain eligible even with identical advertised UDP sockets.
            let mut advertised = record(&server, listen);
            advertised.libp2p.as_mut().unwrap().port = 9;
            let signature = server.libp2p_transport().sign_address(&advertised, "kaspa-simnet").unwrap();
            advertised.libp2p.as_mut().unwrap().signature = signature;
            addresses.lock().add_address(advertised);
            servers.push(server);
        }
        let manager = ConnectionManager::new(client.clone(), servers.len().saturating_mul(2), 0, &[], 16511, addresses);
        // Excess transport work is deferred to the normal 30-second manager tick.
        let completed = tokio::time::timeout(Duration::from_secs(90), async {
            while client.active_peers_len() != servers.len() {
                if store.iterator().any(|row| row.unwrap().1.connection_failed_count > 1) {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            true
        })
        .await;
        manager.stop().await;
        assert!(matches!(completed, Ok(true)), "connected {}/{} libp2p candidates", client.active_peers_len(), servers.len());
        assert!(client.active_peers().iter().all(|peer| peer.libp2p_id().is_some()));
        assert!(store.iterator().all(|row| row.unwrap().1.connection_failed_count == 0));
        client.close().await;
        for server in servers {
            server.close().await;
        }
        for relay in relays {
            relay.close().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tcp_is_preferred_and_failed_tcp_falls_back_to_libp2p() {
        let directory = tempfile::tempdir().unwrap();
        let (relay, _) = node(directory.path(), "relay", false).await;
        for tcp in [true, false] {
            let (client, _) = node(directory.path(), &format!("client-{tcp}"), false).await;
            let (server, listen) = node(directory.path(), &format!("server-{tcp}"), tcp).await;
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            reserve_relay(&server, &relay).await;
            let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
            let addresses = address_manager(db);
            addresses.lock().add_address(record(&server, listen));
            let manager = ConnectionManager::new(client.clone(), 2, 0, &[], 16511, addresses);
            wait(|| client.active_peers_len() == 1, 15).await;
            manager.stop().await;
            assert_eq!(client.active_peers()[0].libp2p_id().is_none(), tcp);
            assert_eq!(store.iterator().next().unwrap().unwrap().1.connection_failed_count, 0);
            client.close().await;
            server.close().await;
        }
        relay.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn outbound_libp2p_limit_counts_existing_outbound_but_not_inbound_sessions() {
        for target in [1usize, 3, 8] {
            let directory = tempfile::tempdir().unwrap();
            let (relay, _) = node(directory.path(), "relay", false).await;
            let (client, _) = node(directory.path(), "client", false).await;
            reserve_relay(&client, &relay).await;
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let addresses = address_manager(db);
            let limit = target / 2;
            let mut servers = Vec::new();
            for index in 0..limit.saturating_add(2) {
                let (server, listen) = node(directory.path(), &format!("libp2p-{index}"), false).await;
                reserve_relay(&server, &relay).await;
                let address = record(&server, listen);
                addresses.lock().add_address(address.clone());
                if index < limit.min(2) {
                    client.connect_libp2p_peer(&address.endpoint().unwrap()).await.unwrap();
                }
                servers.push(server);
            }
            for index in 0..target.saturating_sub(limit) {
                // EchoFlowInitializer leaves the Kaspa UUID unset; distinct IPs keep its TCP peer keys distinct.
                let ip = std::net::Ipv4Addr::new(127, 0, 0, u8::try_from(index).unwrap().saturating_add(2));
                let listen = NetAddress::new(ip.into(), free_port_pair().port);
                let (key, id) = load_identity(directory.path().join(format!("tcp-{index}"))).await.unwrap();
                let transport = Arc::new(Libp2pTransport::new(key.clone(), listen.port + 1, true).unwrap());
                let server =
                    Adaptor::bidirectional(listen, Hub::new(), Arc::new(EchoFlowInitializer::new()), Default::default(), transport)
                        .unwrap();
                let mut address = PeerAddress {
                    libp2p: Some(Libp2pMetadata { endpoint_id: id, port: 0, relay_port: 0, nonce: 1, signature: Vec::new() }),
                    ..listen.into()
                };
                let signature = key.sign(&address.signing_bytes("kaspa-simnet")).unwrap();
                address.libp2p.as_mut().unwrap().signature = signature;
                addresses.lock().add_address(address);
                servers.push(server);
            }
            let (incoming, _) = node(directory.path(), "incoming", false).await;
            incoming.connect_libp2p_peer(&client.libp2p_transport().endpoint_address().unwrap()).await.unwrap();
            wait(|| client.active_peers().iter().any(|peer| !peer.is_outbound()), 10).await;
            let manager = ConnectionManager::new(client.clone(), target, 4, &[], 16511, addresses);
            wait(|| client.active_peers().iter().filter(|peer| peer.is_outbound()).count() == target, 25).await;
            manager.stop().await;
            let peers = client.active_peers();
            assert_eq!(peers.iter().filter(|peer| peer.is_outbound() && peer.libp2p_id().is_some()).count(), limit);
            assert_eq!(
                peers.iter().filter(|peer| peer.is_outbound() && peer.libp2p_id().is_none()).count(),
                target.saturating_sub(limit)
            );
            assert_eq!(peers.iter().filter(|peer| !peer.is_outbound() && peer.libp2p_id().is_some()).count(), 1);
            client.close().await;
            incoming.close().await;
            for server in servers {
                server.close().await;
            }
            relay.close().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_libp2p_quota_does_not_charge_skipped_fallbacks_as_peer_failures() {
        let directory = tempfile::tempdir().unwrap();
        let (relay, _) = node(directory.path(), "relay", false).await;
        let (client, _) = node(directory.path(), "client", false).await;
        let mut servers = Vec::new();
        for index in 0..4 {
            let (server, _) = node(directory.path(), &format!("connected-{index}"), false).await;
            reserve_relay(&server, &relay).await;
            client.connect_libp2p_peer(&server.libp2p_transport().endpoint_address().unwrap()).await.unwrap();
            servers.push(server);
        }
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        let addresses = address_manager(db);
        let (extra, listen) = node(directory.path(), "extra", false).await;
        reserve_relay(&extra, &relay).await;
        let extra_address = record(&extra, listen);
        addresses.lock().add_address(extra_address.clone());
        let (unreachable, listen) = node(directory.path(), "unreachable", false).await;
        let mut marker = record(&unreachable, listen);
        marker.libp2p.as_mut().unwrap().port = 0;
        marker.libp2p.as_mut().unwrap().relay_port = 0;
        let signature = unreachable.libp2p_transport().sign_address(&marker, "kaspa-simnet").unwrap();
        marker.libp2p.as_mut().unwrap().signature = signature;
        addresses.lock().add_address(marker.clone());
        let manager = ConnectionManager::new(client.clone(), 8, 0, &[], 16511, addresses);
        // Both records fit in one batch; charging its TCP-only failure means all dial jobs completed.
        wait(
            || {
                store.iterator().any(|row| {
                    let (key, entry) = row.unwrap();
                    key == marker.key() && entry.connection_failed_count >= 2
                })
            },
            15,
        )
        .await;
        manager.stop().await;
        assert_eq!(client.active_peers().iter().filter(|peer| peer.is_outbound()).count(), 4);
        let entries = store.iterator().collect::<Result<HashMap<_, _>, _>>().unwrap();
        assert_eq!(entries[&extra_address.key()].connection_failed_count, 1);
        client.close().await;
        extra.close().await;
        unreachable.close().await;
        for server in servers {
            server.close().await;
        }
        relay.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connected_libp2p_excludes_aliases_but_allows_other_ids_at_the_same_socket() {
        let directory = tempfile::tempdir().unwrap();
        let (relay, _) = node(directory.path(), "relay", false).await;
        let (client, _) = node(directory.path(), "client", false).await;
        let (server, listen) = node(directory.path(), "server", false).await;
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        let addresses = address_manager(db);
        reserve_relay(&server, &relay).await;
        let good = record(&server, listen);
        addresses.lock().add_address(good.clone());
        let manager = ConnectionManager::new(client.clone(), 4, 0, &[], 16511, addresses.clone());
        wait(|| client.active_peers_len() == 1, 15).await;
        let mut alias = good.clone();
        alias.tcp_port = 1;
        let signature = server.libp2p_transport().sign_address(&alias, "kaspa-simnet").unwrap();
        alias.libp2p.as_mut().unwrap().signature = signature;
        addresses.lock().add_address(alias.clone());
        let (other, _) = node(directory.path(), "other", false).await;
        reserve_relay(&other, &relay).await;
        let mut shared_socket = record(&other, NetAddress::new(good.ip, good.libp2p_port().unwrap()));
        let signature = other.libp2p_transport().sign_address(&shared_socket, "kaspa-simnet").unwrap();
        shared_socket.libp2p.as_mut().unwrap().signature = signature;
        addresses.lock().add_address(shared_socket.clone());
        let peers = client.active_peers().into_iter().map(|peer| (peer.key(), peer)).collect();
        manager.handle_outbound_connections(&peers).await;
        let entries = store.iterator().collect::<Result<HashMap<_, _>, _>>().unwrap();
        assert_eq!(entries[&alias.key()].connection_failed_count, 1);
        assert_eq!(entries[&shared_socket.key()].connection_failed_count, 0);
        wait(|| client.active_peers_len() == 2, 10).await;
        manager.stop().await;
        client.close().await;
        server.close().await;
        other.close().await;
        relay.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_relay_is_reselected_without_charging_tcp_failures() {
        let directory = tempfile::tempdir().unwrap();
        let (client, _) = node(directory.path(), "client", false).await;
        let (first, listen1) = node(directory.path(), "relay1", true).await;
        let (second, listen2) = node(directory.path(), "relay2", true).await;
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        let addresses = address_manager(db);
        let records = [record(&first, listen1), record(&second, listen2)];
        for record in &records {
            addresses.lock().add_address(record.clone());
        }
        let manager = ConnectionManager::new(client.clone(), 0, 0, &[], 16511, addresses);
        let transport = client.libp2p_transport();
        let relays = records.each_ref().map(|record| LibP2pAddress {
            address: SocketAddr::new(record.ip.into(), record.relay_port().unwrap()),
            id: record.endpoint_id().unwrap(),
        });
        wait(|| relays.iter().any(|relay| transport.has_relay_reservation(*relay)), 15).await;
        let selected = usize::from(transport.has_relay_reservation(relays[1]));
        if selected == 0 {
            first.close().await;
        } else {
            second.close().await;
        }
        wait(|| transport.has_relay_reservation(relays[1 - selected]), 35).await;
        manager.stop().await;
        let entries = store.iterator().collect::<Result<HashMap<_, _>, _>>().unwrap();
        assert!(entries[&records[selected].key()].consecutive_relay_failures > 0);
        assert_eq!(entries[&records[1 - selected].key()].consecutive_relay_failures, 0);
        assert!(entries.values().all(|entry| entry.connection_failed_count == 1));
        client.close().await;
        if selected == 0 {
            second.close().await;
        } else {
            first.close().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn legacy_tcp_remains_usable_without_any_relay() {
        let directory = tempfile::tempdir().unwrap();
        let (client, _) = node(directory.path(), "client", false).await;
        let ip = local_ip_address::list_afinet_netifas()
            .unwrap()
            .into_iter()
            .map(|(_, ip)| ip)
            .find(|ip| ip.is_ipv4() && !ip.is_loopback())
            .unwrap();
        let tcp = NetAddress::new(ip.into(), free_port_pair().port);
        let (key, _) = load_identity(directory.path().join("tcp-server")).await.unwrap();
        let transport = Arc::new(Libp2pTransport::new(key, tcp.port + 1, true).unwrap());
        let server =
            Adaptor::bidirectional(tcp, Hub::new(), Arc::new(EchoFlowInitializer::new()), Default::default(), transport).unwrap();
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let addresses = address_manager(db);
        addresses.lock().add_address(tcp.into());
        let manager = ConnectionManager::new(client.clone(), 1, 0, &[], 16511, addresses);
        wait(|| client.active_peers_len() == 1, 10).await;
        assert!(client.active_peers()[0].libp2p_id().is_none());
        assert!(client.libp2p_transport().endpoint_address().unwrap().relay.is_none());
        manager.stop().await;
        client.close().await;
        server.close().await;
    }
}
