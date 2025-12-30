use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::atomic::Ordering,
    sync::{atomic::AtomicUsize, Arc},
    time::{Duration, SystemTime},
};

use duration_string::DurationString;
use futures_util::future::{join_all, try_join_all};
use itertools::Itertools;
use kaspa_addressmanager::{AddressManager, NetAddress};
use kaspa_core::{debug, info, warn};
use kaspa_p2p_lib::{common::ProtocolError, ConnectionError, Peer, PeerOutboundType};
use kaspa_perigeemanager::{PerigeeConfig, PerigeeManager};
use kaspa_utils::triggers::SingleTrigger;
use parking_lot::Mutex as ParkingLotMutex;
use rand::{
    seq::{IteratorRandom, SliceRandom},
    thread_rng,
};
use tokio::{
    select,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex as TokioMutex,
    },
    time::{interval, MissedTickBehavior},
};

pub const EVENT_LOOP_TIMER: Duration = Duration::from_secs(30);

pub enum ConnectionManagerEvent {
    Tick(usize),
    Signal,
}

pub struct ConnectionManager {
    p2p_adaptor: Arc<kaspa_p2p_lib::Adaptor>,
    random_graph_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    address_manager: Arc<ParkingLotMutex<AddressManager>>,
    connection_requests: TokioMutex<HashMap<SocketAddr, ConnectionRequest>>,
    force_next_iteration: UnboundedSender<()>,
    shutdown_signal: SingleTrigger,
    tick_counter: AtomicUsize,
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
        let (tx, rx) = unbounded_channel::<()>();
        let perigee_config = perigee_manager.as_ref().map(|pm| pm.clone().lock().config());
        let manager = Arc::new(Self {
            p2p_adaptor,
            random_graph_target,
            inbound_limit,
            address_manager,
            connection_requests: Default::default(),
            force_next_iteration: tx,
            shutdown_signal: SingleTrigger::new(),
            tick_counter: AtomicUsize::new(0),
            dns_seeders,
            default_port,
            perigee_config,
            perigee_manager,
        });

        manager.clone().start_event_loop(rx);
        manager.force_next_iteration.send(()).unwrap();
        manager
    }

    fn start_event_loop(self: Arc<Self>, mut rx: UnboundedReceiver<()>) {
        let mut ticker = interval(EVENT_LOOP_TIMER);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tokio::spawn(async move {
            loop {
                if self.shutdown_signal.trigger.is_triggered() {
                    break;
                }
                select! {
                    _ = ticker.tick() => self.clone().handle_event(ConnectionManagerEvent::Tick(self.tick_counter.fetch_add(1, Ordering::SeqCst))),
                    _ = rx.recv() =>  self.clone().handle_event(ConnectionManagerEvent::Signal),
                    _ = self.shutdown_signal.listener.clone() => break,
                }
                .await;
            }
            debug!("Connection manager event loop exiting");
        });
    }

    async fn should_evaluate_perigee_round(self: &Arc<Self>, event: ConnectionManagerEvent) -> bool {
        let tick_count = match event {
            ConnectionManagerEvent::Tick(count) => Some(count),
            ConnectionManagerEvent::Signal => None,
        };

        tick_count.is_some()
            && self.perigee_manager.is_some()
            && self.perigee_config.as_ref().is_some_and(|pc| pc.round_frequency != 0 && tick_count.unwrap() % pc.round_frequency == 0)
    }

    async fn evaluate_perigee_round(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) -> bool {
        let (to_exploit, to_evict, has_exploited_changed) = {
            let perigee_manager = self.perigee_manager.as_ref().unwrap();
            let mut perigee_manager_guard = perigee_manager.lock();

            if perigee_manager_guard.config().statistics {
                perigee_manager_guard.log_statistics();
            }

            if perigee_manager_guard.is_first_round() {
                let init = self.address_manager.lock().get_perigee_addresses();
                info!("trying to init from {:?}", peer_by_address);
                perigee_manager_guard.set_initial_persistent_peers(
                    init.iter()
                        .filter_map(|addr| peer_by_address.get(&SocketAddr::new(addr.ip.into(), addr.port)))
                        .map(|p| p.key())
                        .take(self.perigee_config.as_ref().unwrap().perigee_outbound_target)
                        .collect(),
                );
            }

            perigee_manager_guard.evaluate_round()
        };

        if has_exploited_changed && self.perigee_config.as_ref().unwrap().persistence {
            let am = &mut self.address_manager.lock();
            am.set_new_perigee_addresses(
                to_exploit
                    .iter()
                    .filter_map(|p| {
                        peer_by_address
                            .values()
                            .find(|peer| peer.key() == *p)
                            .map(|peer| NetAddress::new(peer.net_address().ip().into(), peer.net_address().port()))
                    })
                    .collect(),
            );
        }

        info!(
            "Connection manager: Perigee Round Completed - Exploiting peers: {:?}, Keeping peers: {:?}, Evicting peers: {:?}",
            peer_by_address
                .iter()
                .filter_map(|(addr, p)| if to_exploit.contains(&p.key()) { Some(addr) } else { None })
                .collect::<Vec<&SocketAddr>>(),
            peer_by_address
                .iter()
                .filter_map(|(addr, p)| if p.is_perigee() && !to_exploit.contains(&p.key()) && !to_evict.contains(&p.key()) {
                    Some(addr)
                } else {
                    None
                })
                .collect::<Vec<&SocketAddr>>(),
            peer_by_address
                .iter()
                .filter_map(|(addr, p)| if to_evict.contains(&p.key()) { Some(addr) } else { None })
                .collect::<Vec<&SocketAddr>>()
        );

        let to_terminate = Vec::from_iter(to_evict.iter().filter_map(|p| peer_by_address.values().find(|peer| peer.key() == *p)));

        self.terminate_peers(to_terminate).await;

        true
    }

    async fn setup_new_perigee_round(self: &Arc<Self>) {
        self.perigee_manager.as_ref().unwrap().lock().start_new_round();
    }

    async fn handle_event(self: Arc<Self>, event: ConnectionManagerEvent) {
        debug!("Starting connection loop iteration");

        let peers = self.p2p_adaptor.active_peers();
        let peer_by_address: HashMap<SocketAddr, Peer> = peers.into_iter().map(|peer| (peer.net_address(), peer)).collect();

        let should_eval_perigee = self.should_evaluate_perigee_round(event).await;

        if should_eval_perigee {
            self.evaluate_perigee_round(&peer_by_address).await;
        }
        self.handle_connection_requests(&peer_by_address).await;
        self.handle_outbound_connections(&peer_by_address).await;
        self.handle_inbound_connections(&peer_by_address).await;
        if should_eval_perigee {
            self.setup_new_perigee_round().await;
        }
    }

    pub async fn add_connection_request(&self, address: SocketAddr, is_permanent: bool) {
        // If the request already exists, it resets the attempts count and overrides the `is_permanent` setting.
        self.connection_requests.lock().await.insert(address, ConnectionRequest::new(is_permanent));
        self.force_next_iteration.send(()).unwrap(); // We force the next iteration of the connection loop.
    }

    pub async fn stop(&self) {
        self.shutdown_signal.trigger.trigger()
    }

    async fn handle_connection_requests(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
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
                match self
                    .p2p_adaptor
                    .connect_peer(
                        address.to_string(),
                        if request.is_permanent { PeerOutboundType::Persistent } else { PeerOutboundType::Temporary },
                    )
                    .await
                {
                    Err(err) => {
                        debug!("Failed connecting to peer request: {}, {}", address, err);
                        if request.is_permanent {
                            const MAX_ACCOUNTABLE_ATTEMPTS: u32 = 4;
                            let retry_duration =
                                Duration::from_secs(30u64 * 2u64.pow(min(request.attempts, MAX_ACCOUNTABLE_ATTEMPTS)));
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

    async fn handle_outbound_connections(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let mut active_outbound = HashSet::new();
        let mut num_active_perigee_outbound = 0usize;
        let mut num_active_random_graph_outbound = 0usize;

        for peer in peer_by_address.values() {
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
                info!(
                    "Connection manager: terminating {} excess random graph outbound connections to respect the target of {}",
                    random_graph_overflow, self.random_graph_target
                );
                let to_terminate = active_outbound
                    .iter()
                    .filter_map(|addr| match peer_by_address.get(&SocketAddr::new(addr.ip.into(), addr.port)) {
                        Some(peer) if peer.is_random_graph() => Some(peer),
                        _ => None,
                    })
                    .choose_multiple(&mut thread_rng(), random_graph_overflow);

                self.terminate_peers(to_terminate).await;
            };
            let perigee_overflow = num_active_perigee_outbound.saturating_sub(self.perigee_outbound_target());
            if perigee_overflow > 0 {
                info!(
                    "Connection manager: terminating {} excess perigee outbound connections to respect the target of {}",
                    perigee_overflow,
                    self.perigee_outbound_target()
                );

                let to_terminate_keys = {
                    let mut pm = self.perigee_manager.as_ref().unwrap().lock();
                    pm.trim_peers().into_iter().collect::<HashSet<_>>()
                };

                let to_terminate = peer_by_address.values().filter(|peer| to_terminate_keys.contains(&peer.key())).collect();

                self.terminate_peers(to_terminate).await;
            }
        }

        let mut missing_random_graph_connections = self.random_graph_target.saturating_sub(num_active_random_graph_outbound);

        let mut missing_perigee_connections = missing_connections.saturating_sub(missing_random_graph_connections);

        // Use a boxed ExactSizeIterator so the `else` branch can return the
        // address-manager iterator directly (no extra collect). Only the
        // perigee branch builds a Vec which is necessary to prepend persisted
        // addresses.
        let mut addr_iter = if active_outbound.is_empty() && self.perigee_config.as_ref().is_some_and(|pc| pc.persistence) {
            // On fresh start-up (or on some other full peer clearing event), and with perigee persistence,
            // we prioritize perigee peers saved to the db from some previous round
            let persistent_perigee_addresses = self.address_manager.lock().get_perigee_addresses();

            let exploit_target = self.perigee_config.as_ref().unwrap().exploitation_target;

            // collect the persisted perigee addresses first
            let priorities = persistent_perigee_addresses.iter().take(exploit_target).cloned().collect();

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

            // because we potentially prioritized perigee connections to the start of the addr_iter, we should start with perigee peers.
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
                    "Connection manager: has {}/{} outgoing P2P connections, trying to obtain {} additional connection(s)...",
                    self.outbound_target() - missing_connections,
                    self.outbound_target(),
                    jobs.len(),
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

    async fn handle_inbound_connections(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let active_inbound = peer_by_address.values().filter(|peer| !peer.is_outbound()).collect_vec();
        let active_inbound_len = active_inbound.len();
        if self.inbound_limit >= active_inbound_len {
            return;
        }

        let to_terminate = active_inbound
            .choose_multiple(&mut thread_rng(), active_inbound_len - self.inbound_limit)
            .cloned()
            .collect::<Vec<&Peer>>();
        debug!(
            "Terminating peers: {:?} to respect the inbound limit of {}",
            to_terminate.iter().map(|p| p.net_address()).collect_vec(),
            self.inbound_limit
        );
        self.terminate_peers(to_terminate).await;
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

    pub fn outbound_target(&self) -> usize {
        self.random_graph_target + self.perigee_outbound_target()
    }

    pub fn perigee_outbound_target(&self) -> usize {
        self.perigee_config.as_ref().map_or(0, |config| config.perigee_outbound_target)
    }

    async fn terminate_peers(&self, peers: Vec<&Peer>) {
        let mut futures = Vec::with_capacity(peers.len());
        for peer in peers {
            futures.push(self.p2p_adaptor.terminate(peer.key()));
        }
        join_all(futures).await;
    }
}
