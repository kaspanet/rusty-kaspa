use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use duration_string::DurationString;
use futures_util::future::{join_all, try_join_all};
use itertools::Itertools;
use kaspa_addressmanager::{AddressManager, NetAddress};
use kaspa_core::{debug, info, warn};
use kaspa_p2p_lib::{ConnectionError, Peer, PeerKey, PeerOutboundType, common::ProtocolError};
use kaspa_perigeemanager::{PerigeeConfig, PerigeeManager};
use kaspa_utils::triggers::SingleTrigger;
use parking_lot::Mutex as ParkingLotMutex;
use rand::{
    seq::{IteratorRandom, SliceRandom},
    thread_rng,
};
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle, try_join};

pub const EVENT_LOOP_TIMER: Duration = Duration::from_secs(30);

pub enum ConnectionManagerEvent {
    Tick(usize),
    AddPeer,
}

pub struct ConnectionManager {
    p2p_adaptor: Arc<kaspa_p2p_lib::Adaptor>,
    random_graph_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    address_manager: Arc<ParkingLotMutex<AddressManager>>,
    connection_requests: Arc<TokioMutex<HashMap<SocketAddr, ConnectionRequest>>>,
    shutdown_signal: SingleTrigger,
    tick_counter: Arc<AtomicUsize>,
    perigee_manager: Option<Arc<ParkingLotMutex<PerigeeManager>>>,
    perigee_config: Option<PerigeeConfig>,
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
        random_graph_target: usize,

        // perigee parameters
        perigee_manager: Option<Arc<ParkingLotMutex<PerigeeManager>>>,
        inbound_limit: usize,
        dns_seeders: &'static [&'static str],
        default_port: u16,
        address_manager: Arc<ParkingLotMutex<AddressManager>>,
    ) -> Arc<Self> {
        let perigee_config = perigee_manager.as_ref().map(|pm| pm.clone().lock().config());
        let manager = Arc::new(Self {
            p2p_adaptor,
            random_graph_target,
            inbound_limit,
            address_manager,
            connection_requests: Arc::new(Default::default()),
            shutdown_signal: SingleTrigger::new(),
            tick_counter: Arc::new(AtomicUsize::new(0)),
            dns_seeders,
            default_port,
            perigee_config,
            perigee_manager,
        });

        manager.clone().start_event_loop();
        manager
    }

    fn start_event_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut next_tick = tokio::time::Instant::now();

            loop {
                if self.shutdown_signal.trigger.is_triggered() {
                    break;
                }
                select! {
                    _ = tokio::time::sleep_until(next_tick) => {
                        debug!("Connection manager handling connections");

                        let tick_start = tokio::time::Instant::now();

                        self.clone().handle_event(ConnectionManagerEvent::Tick(
                            self.tick_counter.fetch_add(1, Ordering::SeqCst)
                        )).await;

                        // Calculate next tick deadline
                        next_tick = tick_start + EVENT_LOOP_TIMER;
                        debug!("Connection manager event loop tick completed in {}", DurationString::from(tokio::time::Instant::now().duration_since(tick_start)));
                        debug!("Next connection manager event loop tick scheduled in {}", DurationString::from(next_tick.duration_since(tokio::time::Instant::now())));
                    },
                    _ = self.shutdown_signal.listener.clone() => break,
                }
            }
            debug!("Connection manager event loop exiting");
        });
    }

    fn spawn_initiate_perigee(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) -> JoinHandle<()> {
        let cmgr = self.clone();
        let peer_by_address = peer_by_address.clone();
        tokio::spawn(async move { cmgr.initiate_perigee(&peer_by_address) })
    }

    fn initiate_perigee(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        // Initiate the perigee manager with persisted peers.
        let perigee_manager = self.perigee_manager.as_ref().unwrap();
        let mut perigee_manager_guard = perigee_manager.lock();
        let init = self.address_manager.lock().get_perigee_addresses();
        info!(
            "Connection manager: Initiating perigee with db persisted peers: {:?}",
            init.iter()
                .map(|addr| SocketAddr::new(addr.ip.into(), addr.port).to_string())
                .collect::<Vec<_>>()
                .join(", ")
                .trim_end_matches(", "),
        );
        perigee_manager_guard.set_initial_persistent_peers(
            init.iter()
                .filter_map(|addr| peer_by_address.get(&SocketAddr::new(addr.ip.into(), addr.port)))
                .map(|p| p.key())
                .take(self.perigee_config.as_ref().unwrap().perigee_outbound_target)
                .collect(),
        );
    }

    async fn evaluate_perigee_round(self: &Arc<Self>, peer_by_address: Arc<HashMap<SocketAddr, Peer>>) -> HashSet<PeerKey> {
        debug!("Evaluating perigee round...");
        let (to_leverage, to_evict, has_leveraged_changed) = {
            let mut perigee_manager_guard = self.perigee_manager.as_ref().unwrap().lock();
            if perigee_manager_guard.config().statistics {
                perigee_manager_guard.log_statistics(&peer_by_address);
            }

            perigee_manager_guard.evaluate_round(&peer_by_address)
        };

        // save leveraged peers to db if persistence is enabled and there was a change in leveraged peers.
        if has_leveraged_changed && self.perigee_config.as_ref().unwrap().persistence {
            let am = &mut self.address_manager.lock();

            // Update persisted perigee addresses
            am.set_new_perigee_addresses(to_leverage.iter().map(|pk| pk.sock_addr().into()).collect());
        }

        // Log the results of the perigee round
        if has_leveraged_changed {
            info!(
                "Connection manager: Leveraging perigee peers \n {}",
                to_leverage.iter().map(|pk| pk.sock_addr().to_string()).collect::<Vec<_>>().join(", ").trim_end_matches(", "),
            );
        } else {
            debug!("Connection manager: No changes in leveraged perigee peers");
        }
        if !to_evict.is_empty() {
            info!(
                "Connection manager: Evicting perigee peers: {}",
                to_evict.iter().map(|pk| pk.sock_addr().to_string()).collect::<Vec<_>>().join(", ").trim_end_matches(", "),
            );
        } else {
            debug!("Connection manager: No perigee peers to evict");
        }

        to_evict
    }

    async fn reset_perigee_round(self: &Arc<Self>) {
        if let Some(perigee_manager) = &self.perigee_manager {
            let mut perigee_manager_guard = perigee_manager.lock();
            perigee_manager_guard.start_new_round();
        }
        // This causes potentially a minor lag between the perigee manager round reset and the peer perigee timestamp resets,
        // better to clear this after the perigee manager reset to avoid penalizing fast peers sending data in this short lag.
        self.p2p_adaptor.clear_perigee_timestamps().await;
        debug!("Connection manager: Reset perigee round");
    }

    fn get_peers_by_address(self: &Arc<Self>, include_perigee_data: bool) -> Arc<HashMap<SocketAddr, Peer>> {
        debug!("Getting peers by addresses (include_perigee_data={})", include_perigee_data);
        let peers = self.p2p_adaptor.active_peers(include_perigee_data);
        Arc::new(peers.into_iter().map(|peer| (peer.net_address(), peer)).collect())
    }

    async fn handle_event(self: Arc<Self>, event: ConnectionManagerEvent) {
        match event {
            ConnectionManagerEvent::Tick(tick_count) => {
                let should_initiate_perigee = self.perigee_config.as_ref().is_some_and(|pc| pc.persistence && tick_count == 0);
                let should_activate_perigee =
                    self.perigee_config.as_ref().is_some_and(|pc| tick_count % pc.round_frequency == 0 && tick_count != 0);
                if should_initiate_perigee {
                    let mut peer_by_address = self.get_peers_by_address(false);
                    // First, we await populating outbound connections.
                    self.handle_outbound_connections(peer_by_address.clone(), HashSet::new()).await;

                    // Now, we can reinstate peer_by_address to include the newly connected peers
                    peer_by_address = self.get_peers_by_address(false); // don't need the data to init

                    // Continue with the congruent connection handling.
                    try_join!(
                        self.spawn_initiate_perigee(&peer_by_address),
                        self.spawn_handle_connection_requests(peer_by_address.clone()),
                        self.spawn_handle_inbound_connections(peer_by_address.clone()),
                    )
                    .unwrap();
                } else if should_activate_perigee {
                    let peer_by_address = self.get_peers_by_address(true);

                    // This is a round where perigee should be evaluated and processed.

                    // We await this (not spawn), so that `spawn_handle_outbound_connections` is called after the perigee round evaluation is executed,
                    let peers_evicted = self.evaluate_perigee_round(peer_by_address.clone()).await;

                    // Reset the perigee round state.
                    self.reset_perigee_round().await;

                    // Continue with the regular congruent connection handling.
                    try_join!(
                        self.spawn_handle_outbound_connections(peer_by_address.clone(), peers_evicted),
                        self.spawn_handle_inbound_connections(peer_by_address.clone()),
                        self.spawn_handle_connection_requests(peer_by_address.clone()),
                    )
                    .unwrap();
                } else {
                    let peer_by_address = self.get_peers_by_address(false);
                    try_join!(
                        self.spawn_handle_outbound_connections(peer_by_address.clone(), HashSet::new()),
                        self.spawn_handle_inbound_connections(peer_by_address.clone()),
                        self.spawn_handle_connection_requests(peer_by_address.clone()),
                    )
                    .unwrap();
                }
            }
            ConnectionManagerEvent::AddPeer => {
                let peer_by_address = self.get_peers_by_address(false);
                // We only need to handle connection requests for this event.
                self.spawn_handle_connection_requests(peer_by_address).await.unwrap();
            }
        }
    }

    pub async fn add_connection_requests(self: Arc<Self>, requests: Vec<(SocketAddr, bool)>) {
        // If the request already exists, it resets the attempts count and overrides the `is_permanent` setting.
        let mut connection_requests = self.connection_requests.lock().await;
        for (address, is_permanent) in requests {
            connection_requests.insert(address, ConnectionRequest::new(is_permanent));
        }
        drop(connection_requests);
        self.handle_event(ConnectionManagerEvent::AddPeer).await;
    }

    pub async fn stop(&self) {
        self.shutdown_signal.trigger.trigger()
    }

    fn spawn_handle_connection_requests(self: &Arc<Self>, peer_by_address: Arc<HashMap<SocketAddr, Peer>>) -> JoinHandle<()> {
        let cmgr = self.clone();
        tokio::spawn(async move { cmgr.handle_connection_requests(peer_by_address).await })
    }

    async fn handle_connection_requests(self: &Arc<Self>, peer_by_address: Arc<HashMap<SocketAddr, Peer>>) {
        let mut requests = self.connection_requests.lock().await;
        let mut new_requests = HashMap::with_capacity(requests.len());
        for (address, request) in requests.iter() {
            let address = *address;
            let request = request.clone();
            let is_connected = peer_by_address.contains_key(&address);
            if is_connected && !request.is_permanent {
                // The peer is connected and the request is not permanent - no need to keep the request
                continue;
            }

            if !is_connected && request.next_attempt <= SystemTime::now() {
                debug!("Connecting to peer request {}", address);
                match self.p2p_adaptor.connect_peer(address.to_string(), PeerOutboundType::UserSupplied).await {
                    Err(err) => {
                        debug!("Failed connecting to peer request: {}, {}", address, err);
                        if request.is_permanent {
                            const MAX_ACCOUNTABLE_ATTEMPTS: u32 = 4;
                            let retry_duration = Duration::from_secs(
                                EVENT_LOOP_TIMER.as_secs() * 2u64.pow(min(request.attempts, MAX_ACCOUNTABLE_ATTEMPTS)),
                            );
                            debug!("Will retry peer request {} in {}", address, DurationString::from(retry_duration));
                            new_requests.insert(
                                address,
                                ConnectionRequest {
                                    next_attempt: SystemTime::now() + retry_duration,
                                    attempts: request.attempts + 1,
                                    is_permanent: true,
                                },
                            );
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

    fn spawn_handle_outbound_connections(
        self: &Arc<Self>,
        peer_by_address: Arc<HashMap<SocketAddr, Peer>>,
        to_terminate: HashSet<PeerKey>,
    ) -> JoinHandle<()> {
        let cmgr = self.clone();
        tokio::spawn(async move { cmgr.handle_outbound_connections(peer_by_address, to_terminate).await })
    }

    async fn handle_outbound_connections(
        self: &Arc<Self>,
        peer_by_address: Arc<HashMap<SocketAddr, Peer>>,
        to_terminate: HashSet<PeerKey>,
    ) {
        debug!("Handling outbound connections...");

        let mut active_outbound = HashSet::new();
        let mut num_active_perigee_outbound = 0usize;
        let mut num_active_random_graph_outbound = 0usize;

        let peers_by_address = if !to_terminate.is_empty() {
            // Create a filtered view of peer_by_address without the terminated peers.
            let filtered_peer_by_address = Arc::new(
                peer_by_address
                    .iter()
                    // Filter out peers that were just terminated.
                    .filter(|(_, peer)| !to_terminate.contains(&peer.key()))
                    .map(|(addr, peer)| (*addr, peer.clone()))
                    .collect::<HashMap<SocketAddr, Peer>>(),
            );

            // Terminate peers passed explicitly.
            self.terminate_peers(to_terminate.into_iter()).await;

            filtered_peer_by_address
        } else {
            peer_by_address.clone()
        };

        for peer in peers_by_address.values() {
            match peer.outbound_type() {
                Some(obt) => {
                    let net_addr = NetAddress::new(peer.net_address().ip().into(), peer.net_address().port());
                    active_outbound.insert(net_addr);
                    match obt {
                        PeerOutboundType::Perigee => num_active_perigee_outbound += 1,
                        PeerOutboundType::RandomGraph => num_active_random_graph_outbound += 1,
                        _ => continue,
                    };
                }
                None => continue,
            };
        }

        let num_active_outbound_respecting_peers = num_active_perigee_outbound + num_active_random_graph_outbound;

        info!(
            "Connection manager: outbound respecting connections: {}/{} (Perigee: {}/{}, RandomGraph: {}/{}); Others: {} )",
            num_active_outbound_respecting_peers,
            self.outbound_target(),
            num_active_perigee_outbound,
            self.perigee_outbound_target(),
            num_active_random_graph_outbound,
            self.random_graph_target,
            active_outbound.len().saturating_sub(num_active_outbound_respecting_peers)
        );

        let mut missing_connections = self.outbound_target().saturating_sub(num_active_outbound_respecting_peers);

        if missing_connections == 0 {
            let random_graph_overflow = num_active_random_graph_outbound.saturating_sub(self.random_graph_target);
            if random_graph_overflow > 0 {
                let to_terminate_keys = active_outbound
                    .iter()
                    .filter_map(|addr| match peer_by_address.get(&SocketAddr::new(addr.ip.into(), addr.port)) {
                        Some(peer) if peer.is_random_graph() => Some(peer.key()),
                        _ => None,
                    })
                    .choose_multiple(&mut thread_rng(), random_graph_overflow);

                info!(
                    "Connection manager: terminating {} excess random graph outbound connections to respect the target of {}",
                    random_graph_overflow, self.random_graph_target
                );
                self.terminate_peers(to_terminate_keys.into_iter()).await;
            };
            let perigee_overflow = num_active_perigee_outbound.saturating_sub(self.perigee_outbound_target());
            if perigee_overflow > 0 {
                let to_terminate_keys = {
                    let mut pm = self.perigee_manager.as_ref().unwrap().lock();
                    pm.trim_peers(peers_by_address).into_iter().collect::<HashSet<_>>()
                };

                info!(
                    "Connection manager: terminating {} excess perigee outbound connections to respect the target of {}",
                    perigee_overflow,
                    self.perigee_outbound_target()
                );
                self.terminate_peers(to_terminate_keys.into_iter()).await;
            }
        }

        let mut missing_random_graph_connections = self.random_graph_target.saturating_sub(num_active_random_graph_outbound);

        let mut missing_perigee_connections = missing_connections.saturating_sub(missing_random_graph_connections);

        // Use a boxed ExactSizeIterator so the `else` branch can return the
        // address-manager iterator directly (no extra collect). Only the
        // perigee branch builds a Vec which is necessary to prepend persisted
        // addresses.
        let mut addr_iter = if active_outbound.is_empty() && self.perigee_config.as_ref().is_some_and(|pc| pc.persistence) {
            // On fresh start-up (or some other full peer clearing event), and with perigee persistence,
            // we prioritize perigee peers saved to the DB from some previous round.
            let persistent_perigee_addresses = self.address_manager.lock().get_perigee_addresses();

            let leverage_target = self.perigee_config.as_ref().unwrap().leverage_target;

            // Collect the persisted perigee addresses first.
            let priorities = persistent_perigee_addresses.into_iter().take(leverage_target).collect();

            self.address_manager.lock().iterate_prioritized_random_addresses(priorities, active_outbound)
        } else {
            self.address_manager.lock().iterate_prioritized_random_addresses(vec![], active_outbound)
        };

        let mut progressing = true;
        let mut connecting = true;
        while connecting && missing_connections > 0 {
            if self.shutdown_signal.trigger.is_triggered() {
                return;
            }

            let mut addrs_to_connect = Vec::with_capacity(missing_connections);
            let mut jobs = Vec::with_capacity(missing_connections);
            let mut random_graph_addrs = HashSet::new();
            let mut perigee_addrs = HashSet::new();

            // Because we potentially prioritized perigee connections to the start of addr_iter, we should start with perigee peers.
            for _ in 0..missing_perigee_connections {
                let Some(net_addr) = addr_iter.next() else {
                    connecting = false;
                    break;
                };
                let socket_addr = SocketAddr::new(net_addr.ip.into(), net_addr.port).to_string();
                debug!("Connecting to {}", &socket_addr);
                addrs_to_connect.push(net_addr);
                perigee_addrs.insert(net_addr);
                jobs.push(self.p2p_adaptor.connect_peer(socket_addr.clone(), PeerOutboundType::Perigee));
            }

            for _ in 0..missing_random_graph_connections {
                let Some(net_addr) = addr_iter.next() else {
                    connecting = false;
                    break;
                };
                let socket_addr = SocketAddr::new(net_addr.ip.into(), net_addr.port).to_string();
                debug!("Connecting to {}", &socket_addr);
                addrs_to_connect.push(net_addr);
                random_graph_addrs.insert(net_addr);
                jobs.push(self.p2p_adaptor.connect_peer(socket_addr.clone(), PeerOutboundType::RandomGraph));
            }

            if progressing && !jobs.is_empty() {
                // Log only if progress was made
                info!(
                    "Connection manager: trying to obtain {} additional outbound connection(s) ({}/{}).",
                    jobs.len(),
                    self.outbound_target() - missing_connections,
                    self.outbound_target(),
                );
                progressing = false;
            } else {
                debug!(
                    "Connection manager: outgoing: {}/{} , connecting: {}, iterator: {}",
                    self.outbound_target() - missing_connections,
                    self.outbound_target(),
                    jobs.len(),
                    addr_iter.len(),
                );
            }
            for (res, net_addr) in (join_all(jobs).await).into_iter().zip(addrs_to_connect) {
                match res {
                    Ok(_) => {
                        self.address_manager.lock().mark_connection_success(net_addr);
                        if perigee_addrs.contains(&net_addr) {
                            missing_perigee_connections -= 1;
                        } else {
                            missing_random_graph_connections -= 1;
                        }
                        missing_connections -= 1;
                        progressing = true;
                    }
                    Err(ConnectionError::ProtocolError(ProtocolError::PeerAlreadyExists(_))) => {
                        // We avoid marking the existing connection as connection failure
                        debug!("Failed connecting to {:?}, peer already exists", net_addr);
                    }
                    Err(err) => {
                        debug!("Failed connecting to {:?}, err: {}", net_addr, err);
                        self.address_manager.lock().mark_connection_failure(net_addr);
                    }
                }
            }
        }

        if missing_connections > 0 && !self.dns_seeders.is_empty() {
            if missing_connections > self.outbound_target() / 2 {
                // If we are missing more than half of our target, query all in parallel.
                // This will always be the case on new node start-up and is the most resilient strategy in such a case.
                self.dns_seed_many(self.dns_seeders.len()).await;
            } else {
                // Try to obtain at least twice the number of missing connections
                self.dns_seed_with_address_target(2 * missing_connections).await;
            }
        }
    }

    fn spawn_handle_inbound_connections(self: &Arc<Self>, peer_by_address: Arc<HashMap<SocketAddr, Peer>>) -> JoinHandle<()> {
        let cmgr = self.clone();
        tokio::spawn(async move { cmgr.handle_inbound_connections(peer_by_address).await })
    }

    async fn handle_inbound_connections(self: &Arc<Self>, peer_by_address: Arc<HashMap<SocketAddr, Peer>>) {
        let active_inbound = peer_by_address.values().filter(|peer| !peer.is_outbound()).collect_vec();
        let active_inbound_len = active_inbound.len();

        info!("Connection manager: inbound connections: {}/{}", active_inbound_len, self.inbound_limit,);

        if self.inbound_limit >= active_inbound_len {
            return;
        }

        let to_terminate = active_inbound.choose_multiple(&mut thread_rng(), active_inbound_len - self.inbound_limit);

        info!("Connection manager: terminating {} inbound peers", to_terminate.len());

        self.terminate_peers(to_terminate.into_iter().map(|peer| peer.key())).await;
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
            if addrs_len >= min_addresses_to_fetch {
                break;
            } else {
                min_addresses_to_fetch -= addrs_len;
            }
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
            amgr_lock.add_address(NetAddress::new(addr.ip().into(), addr.port()));
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
        for peer in self.p2p_adaptor.active_peers(false) {
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

    pub fn outbound_target(&self) -> usize {
        self.random_graph_target + self.perigee_outbound_target()
    }

    pub fn perigee_outbound_target(&self) -> usize {
        self.perigee_config.as_ref().map_or(0, |config| config.perigee_outbound_target)
    }

    async fn terminate_peers(&self, peer_keys: impl IntoIterator<Item = PeerKey>) {
        let mut futures = Vec::new();
        for peer_key in peer_keys.into_iter() {
            debug!("Terminating peer: {}", peer_key.sock_addr());
            futures.push(self.p2p_adaptor.terminate(peer_key));
        }
        join_all(futures).await;
    }
}
