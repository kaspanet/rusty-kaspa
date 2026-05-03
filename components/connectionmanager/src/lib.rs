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
use kaspa_p2p_lib::{ConnectionError, Peer, common::ProtocolError};
use kaspa_utils::{
    networking::{PeerEndpoint, PeerEndpointResolveError},
    triggers::SingleTrigger,
};
use parking_lot::Mutex as ParkingLotMutex;
use rand::{seq::SliceRandom, thread_rng};
use thiserror::Error;
use tokio::{
    select,
    sync::{
        Mutex as TokioMutex,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
    time::{MissedTickBehavior, interval},
};

pub mod hostname;
// Crate-root re-exports are restricted to types that have external
// callers (kaspad daemon construction, kaspa-testing-integration metric
// scraping, kaspa-p2p-flows resolver injection). Internal types
// (`HostnameDelta`, `HostnameMetrics`, `HostnameRegistry`,
// `HostnameRequest`, `PendingRefresh`, `ResolutionsTotal`,
// `ResolveStatus`, `ResolveTrigger`, `StaleReason`) remain reachable
// via the `kaspa_connectionmanager::hostname::<Name>` path so
// downstream test code can still construct or match on them when
// needed.
use hostname::{HostnameMetrics, HostnameRegistry, ResolveStatus, ResolveTrigger};
pub use hostname::{HostnameMetricsSnapshot, HostnameResolver, TokioHostnameResolver};

/// Errors raised by [`ConnectionManager::add_endpoint_request`].
#[derive(Error, Debug)]
pub enum AddEndpointError {
    /// DNS resolution failed for the hostname variant.
    #[error("hostname resolution failed: {0}")]
    Resolve(#[from] PeerEndpointResolveError),
    /// Hostname resolved to zero socket addresses.
    #[error("hostname `{0}` resolved to zero addresses")]
    EmptyResult(String),
}

pub struct ConnectionManager {
    p2p_adaptor: Arc<kaspa_p2p_lib::Adaptor>,
    outbound_target: usize,
    inbound_limit: usize,
    dns_seeders: &'static [&'static str],
    default_port: u16,
    address_manager: Arc<ParkingLotMutex<AddressManager>>,
    connection_requests: TokioMutex<HashMap<SocketAddr, ConnectionRequest>>,
    /// Hostname-keyed state for hostname-origin entries. Sibling map to
    /// `connection_requests`; never used as a routing key. Disjoint from the
    /// address manager's gossip economy.
    hostname_state: TokioMutex<HostnameRegistry>,
    /// Periodic re-resolution interval. `Duration::ZERO` disables the
    /// background refresh task; dial-failure-triggered re-resolution still
    /// runs in either case.
    hostname_refresh_interval: Duration,
    /// DNS resolver dependency. Production code wires
    /// [`TokioHostnameResolver`]; tests substitute a fake.
    resolver: Arc<dyn HostnameResolver>,
    /// Hostnames currently being registered by an in-flight
    /// `add_endpoint_request` call. Used to short-circuit concurrent
    /// duplicate registrations of the same host so the un-locked
    /// resolve phase never races a sibling caller's upsert.
    pending_registrations: ParkingLotMutex<HashSet<Arc<str>>>,
    /// Counters for `peer_hostname_resolutions_total`. Gauges are computed
    /// from the live `hostname_state` at snapshot time.
    hostname_metrics: Arc<HostnameMetrics>,
    force_next_iteration: UnboundedSender<()>,
    /// Wake-up channel for the hostname refresh task; kept separate from
    /// `force_next_iteration` so the dial loop and the refresh loop never
    /// poke each other unnecessarily.
    force_hostname_refresh: UnboundedSender<()>,
    shutdown_signal: SingleTrigger,
}

#[derive(Clone, Debug)]
struct ConnectionRequest {
    next_attempt: SystemTime,
    is_permanent: bool,
    attempts: u32,
    /// `Some(host)` iff the entry was inserted by hostname resolution. The
    /// dial loop uses this to mark the corresponding hostname stale on dial
    /// failure so the refresh task re-resolves it on the next tick.
    hostname_origin: Option<Arc<str>>,
}

impl ConnectionRequest {
    fn new(is_permanent: bool) -> Self {
        Self { next_attempt: SystemTime::now(), is_permanent, attempts: 0, hostname_origin: None }
    }

    fn new_hostname_origin(is_permanent: bool, host: Arc<str>) -> Self {
        Self { next_attempt: SystemTime::now(), is_permanent, attempts: 0, hostname_origin: Some(host) }
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
        hostname_refresh_interval: Duration,
        resolver: Arc<dyn HostnameResolver>,
    ) -> Arc<Self> {
        let (tx, rx) = unbounded_channel::<()>();
        let (refresh_tx, refresh_rx) = unbounded_channel::<()>();
        let manager = Arc::new(Self {
            p2p_adaptor,
            outbound_target,
            inbound_limit,
            address_manager,
            connection_requests: Default::default(),
            hostname_state: TokioMutex::new(HostnameRegistry::new()),
            hostname_refresh_interval,
            resolver,
            pending_registrations: Default::default(),
            hostname_metrics: Arc::new(HostnameMetrics::default()),
            force_next_iteration: tx,
            force_hostname_refresh: refresh_tx,
            shutdown_signal: SingleTrigger::new(),
            dns_seeders,
            default_port,
        });
        manager.clone().start_event_loop(rx);
        manager.clone().start_hostname_refresh_loop(refresh_rx);
        manager.force_next_iteration.send(()).unwrap();
        if hostname::refresh_enabled(manager.hostname_refresh_interval) {
            info!("Connection manager: hostname refresh interval = {}", DurationString::from(manager.hostname_refresh_interval));
        } else {
            info!("Connection manager: hostname refresh disabled (interval = 0)");
        }
        manager
    }

    fn start_event_loop(self: Arc<Self>, mut rx: UnboundedReceiver<()>) {
        let mut ticker = interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tokio::spawn(async move {
            loop {
                if self.shutdown_signal.trigger.is_triggered() {
                    break;
                }
                select! {
                    _ = rx.recv() => self.clone().handle_event().await,
                    _ = ticker.tick() => self.clone().handle_event().await,
                    _ = self.shutdown_signal.listener.clone() => break,
                }
            }
            debug!("Connection manager event loop exiting");
        });
    }

    async fn handle_event(self: Arc<Self>) {
        debug!("Starting connection loop iteration");
        let peers = self.p2p_adaptor.active_peers();
        let peer_by_address: HashMap<SocketAddr, Peer> = peers.into_iter().map(|peer| (peer.net_address(), peer)).collect();

        self.handle_connection_requests(&peer_by_address).await;
        self.handle_outbound_connections(&peer_by_address).await;
        self.handle_inbound_connections(&peer_by_address).await;
    }

    /// Insert a numeric IP request directly. Crate-internal: external
    /// callers (FlowService startup, RPC `AddPeer`) go through
    /// `add_endpoint_request`, which routes the IP-only `Address`
    /// variant here and registers hostnames separately. Demoting to
    /// `pub(crate)` prevents future consumers from bypassing the
    /// hostname-aware bookkeeping (`pending_registrations` dedup,
    /// `hostname_origin` back-reference, dial-failure mark-stale).
    pub(crate) async fn add_connection_request(&self, address: SocketAddr, is_permanent: bool) {
        // If the request already exists, it resets the attempts count and overrides the `is_permanent` setting.
        self.connection_requests.lock().await.insert(address, ConnectionRequest::new(is_permanent));
        self.force_next_iteration.send(()).unwrap(); // We force the next iteration of the connection loop.
    }

    /// Insert a peer endpoint into the connection-request set. `Address`
    /// variants short-circuit to the existing IP-keyed path. `Hostname`
    /// variants resolve synchronously (so the caller learns about typos
    /// immediately), record state in the hostname registry, and seed
    /// `connection_requests` with one entry per resolved socket address.
    ///
    /// Concurrent registrations for the same hostname are de-duplicated
    /// via the `pending_registrations` set: the second
    /// caller short-circuits with `Ok(())` rather than racing the
    /// first call's resolve+upsert. The DNS lookup runs outside the
    /// `hostname_state` lock so the registry stays available to
    /// periodic-refresh and metric-snapshot consumers during the
    /// (up to [`kaspa_utils::networking::PEER_ENDPOINT_RESOLVE_TIMEOUT`]-long) resolve.
    pub async fn add_endpoint_request(
        &self,
        endpoint: PeerEndpoint,
        is_permanent: bool,
        default_port: u16,
    ) -> Result<(), AddEndpointError> {
        match endpoint {
            PeerEndpoint::Address(addr) => {
                let socket = SocketAddr::from(addr.normalize(default_port));
                self.add_connection_request(socket, is_permanent).await;
                Ok(())
            }
            PeerEndpoint::Hostname { host, port } => {
                let port = port.unwrap_or(default_port);
                let host_arc: Arc<str> = Arc::from(host.as_str());

                // Dedup pass 1: pending-registrations set. Concurrent
                // same-host caller gets `Ok(())` and short-circuits.
                if !self.pending_registrations.lock().insert(host_arc.clone()) {
                    return Ok(());
                }
                // RAII cleanup: any return path below clears the
                // pending entry exactly once, including panics in the
                // resolve / upsert phases.
                let _pending_guard = PendingRegistrationGuard { pending: &self.pending_registrations, host: host_arc.clone() };

                // Dedup pass 2: registry already has the host.
                if self.hostname_state.lock().await.contains(&host) {
                    return Ok(());
                }

                // Resolve outside any registry-related lock so other
                // consumers of `hostname_state` are not blocked.
                let resolved = match self.resolver.resolve(&host, port).await {
                    Ok(addrs) => {
                        self.hostname_metrics.record(ResolveTrigger::Initial, ResolveStatus::Ok);
                        addrs
                    }
                    Err(e) => {
                        self.hostname_metrics.record(ResolveTrigger::Initial, ResolveStatus::Failed);
                        return Err(AddEndpointError::Resolve(e));
                    }
                };
                if resolved.is_empty() {
                    return Err(AddEndpointError::EmptyResult(host));
                }
                let initial: HashSet<SocketAddr> = resolved.iter().copied().collect();
                let host_arc_inserted = {
                    let mut hostname_state = self.hostname_state.lock().await;
                    hostname_state.upsert(&host, port, is_permanent, initial)
                };
                info!("addpeer: resolved {host} -> {resolved:?}");
                let mut requests = self.connection_requests.lock().await;
                for addr in resolved {
                    requests.insert(addr, ConnectionRequest::new_hostname_origin(is_permanent, host_arc_inserted.clone()));
                }
                drop(requests);
                let _ = self.force_next_iteration.send(());
                Ok(())
            }
        }
    }

    fn start_hostname_refresh_loop(self: Arc<Self>, mut refresh_rx: UnboundedReceiver<()>) {
        let cadence = self.hostname_refresh_interval;
        let periodic_enabled = hostname::refresh_enabled(cadence);
        tokio::spawn(async move {
            // The periodic ticker is constructed only when periodic
            // refresh is enabled. When `cadence` is `Duration::ZERO` the
            // ticker arm pends forever (`std::future::pending`); the
            // dial-failure consumer (`refresh_rx`) keeps running so a
            // hostname-origin dial failure still triggers re-resolution
            // -- which is the documented contract on `hostname_refresh_interval`.
            let mut ticker = if periodic_enabled {
                let mut t = interval(cadence);
                t.set_missed_tick_behavior(MissedTickBehavior::Delay);
                // Skip the immediate-fire tick that `interval` emits at construction.
                t.tick().await;
                Some(t)
            } else {
                None
            };
            loop {
                if self.shutdown_signal.trigger.is_triggered() {
                    break;
                }
                select! {
                    _ = next_periodic_tick(&mut ticker) => self.clone().refresh_hostnames(ResolveTrigger::Periodic).await,
                    _ = refresh_rx.recv() => self.clone().refresh_hostnames(ResolveTrigger::DialFailure).await,
                    _ = self.shutdown_signal.listener.clone() => break,
                }
            }
            debug!("Hostname refresh loop exiting");
        });
    }

    /// Resolve eligible hostname entries through the configured
    /// resolver and reconcile each delta into `connection_requests`.
    ///
    /// Eligibility is gated on `last_refresh + cadence <= now` (or
    /// the entry is marked stale via [`HostnameRegistry::mark_stale`]);
    /// hosts whose cadence window has not elapsed are skipped on this
    /// tick, so a dial-failure wakeup re-resolves only the hosts the
    /// dial loop actually flagged.
    ///
    /// DNS lookups run outside the `hostname_state` lock and in
    /// parallel via `join_all`, so a slow resolver does not block
    /// other consumers of the registry mutex (`add_endpoint_request`,
    /// `host_for_socket`, metric snapshots). Per-result metric
    /// recording runs after the resolves complete and before the
    /// re-acquisition of the registry lock.
    pub async fn refresh_hostnames(self: Arc<Self>, trigger: ResolveTrigger) {
        // Phase 1: snapshot eligible hosts under lock; lock is dropped
        // before any DNS work begins.
        let snapshots = {
            let state = self.hostname_state.lock().await;
            state.pending_refreshes(self.hostname_refresh_interval)
        };
        if snapshots.is_empty() {
            return;
        }
        // Phase 2: resolve concurrently outside the registry lock.
        let resolves = snapshots.into_iter().map(|(host, port, _prev)| {
            let resolver = self.resolver.clone();
            async move {
                let result = resolver.resolve(&host, port).await;
                (host, result)
            }
        });
        let results = join_all(resolves).await;
        // Phase 3a: record per-result metrics outside the registry lock.
        for (_, result) in &results {
            let status = if result.is_ok() { ResolveStatus::Ok } else { ResolveStatus::Failed };
            self.hostname_metrics.record(trigger, status);
        }
        // Phase 3b: re-acquire lock, apply results, capture permanence
        // for delta reconciliation in one pass.
        let (deltas, permanence) = {
            let mut state = self.hostname_state.lock().await;
            let deltas = state.apply_refresh_results(results);
            let permanence: HashMap<Arc<str>, bool> = state.iter().map(|(host, req)| (host.clone(), req.is_permanent)).collect();
            (deltas, permanence)
        };
        if deltas.is_empty() {
            return;
        }
        // Phase 4: reconcile deltas into connection_requests outside
        // the registry lock.
        let mut requests = self.connection_requests.lock().await;
        for delta in &deltas {
            info!("addpeer: {} +{} new, -{} removed", delta.host, delta.added.len(), delta.removed.len());
            // `permanence` is built under the same `hostname_state` lock
            // acquisition that produced `deltas` (Phase 3 above), and
            // `apply_refresh_results` only emits a delta for a host that
            // exists in the registry -- so every delta's host is
            // structurally present in `permanence`. A missing key is a
            // future-regression signal (e.g. a refactor splits the
            // permanence-build from the apply across separate lock
            // acquisitions); fail loudly rather than silently demoting
            // the entry to `is_permanent: false` (which the dial loop
            // would then drop on first successful connect).
            let is_permanent = permanence.get(&delta.host).copied().expect(
                "delta host must be in permanence map (built under same hostname_state lock acquisition as apply_refresh_results)",
            );
            for addr in &delta.added {
                requests.insert(*addr, ConnectionRequest::new_hostname_origin(is_permanent, delta.host.clone()));
            }
            for addr in &delta.removed {
                requests.remove(addr);
            }
        }
        drop(requests);
        let _ = self.force_next_iteration.send(());
    }

    pub async fn stop(&self) {
        self.shutdown_signal.trigger.trigger()
    }

    /// Point-in-time snapshot of the hostname-related metrics (counters
    /// from the live `HostnameMetrics`, gauges computed from the live
    /// `HostnameRegistry`). Suitable for export to a metrics endpoint or
    /// the existing `GetMetrics` RPC payload.
    pub async fn hostname_metrics_snapshot(&self) -> HostnameMetricsSnapshot {
        let mut snapshot = self.hostname_metrics.snapshot();
        let state = self.hostname_state.lock().await;
        snapshot.active = state.len() as u64;
        snapshot.resolved_addrs = state.total_resolved_addrs() as u64;
        snapshot
    }

    async fn handle_connection_requests(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let mut requests = self.connection_requests.lock().await;
        let mut new_requests = HashMap::with_capacity(requests.len());
        let mut stale_hosts: HashSet<Arc<str>> = HashSet::new();
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
                match self.p2p_adaptor.connect_peer(address.to_string()).await {
                    Err(err) => {
                        debug!("Failed connecting to peer request: {}, {}", address, err);
                        if let Some(host) = request.hostname_origin.clone() {
                            stale_hosts.insert(host);
                        }
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
                                    hostname_origin: request.hostname_origin.clone(),
                                },
                            );
                        }
                    }
                    Ok(_) if request.is_permanent => {
                        // Permanent requests are kept forever; preserve hostname back-reference.
                        let mut fresh = ConnectionRequest::new(true);
                        fresh.hostname_origin = request.hostname_origin.clone();
                        new_requests.insert(address, fresh);
                    }
                    Ok(_) => {}
                }
            } else {
                new_requests.insert(address, request);
            }
        }

        *requests = new_requests;
        drop(requests);

        if !stale_hosts.is_empty() {
            let mut hostname_state = self.hostname_state.lock().await;
            for host in &stale_hosts {
                hostname_state.mark_stale(host);
            }
            // Best-effort wakeup of the refresh task; if the channel is closed
            // (shutdown), the next periodic tick still picks up the stale flag.
            let _ = self.force_hostname_refresh.send(());
        }
    }

    async fn handle_outbound_connections(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let active_outbound: HashSet<kaspa_addressmanager::NetAddress> =
            peer_by_address.values().filter(|peer| peer.is_outbound()).map(|peer| peer.net_address().into()).collect();
        if active_outbound.len() >= self.outbound_target {
            return;
        }

        let mut missing_connections = self.outbound_target - active_outbound.len();
        let mut addr_iter = self.address_manager.lock().iterate_prioritized_random_addresses(active_outbound);
        let mut progressing = true;
        let mut connecting = true;
        while connecting && missing_connections > 0 {
            if self.shutdown_signal.trigger.is_triggered() {
                return;
            }
            let mut addrs_to_connect = Vec::with_capacity(missing_connections);
            let mut jobs = Vec::with_capacity(missing_connections);
            for _ in 0..missing_connections {
                let Some(net_addr) = addr_iter.next() else {
                    connecting = false;
                    break;
                };
                let socket_addr = SocketAddr::new(net_addr.ip.into(), net_addr.port).to_string();
                debug!("Connecting to {}", &socket_addr);
                addrs_to_connect.push(net_addr);
                jobs.push(self.p2p_adaptor.connect_peer(socket_addr.clone()));
            }

            if progressing && !jobs.is_empty() {
                // Log only if progress was made
                info!(
                    "Connection manager: has {}/{} outgoing P2P connections, trying to obtain {} additional connection(s)...",
                    self.outbound_target - missing_connections,
                    self.outbound_target,
                    jobs.len(),
                );
                progressing = false;
            } else {
                debug!(
                    "Connection manager: outgoing: {}/{} , connecting: {}, iterator: {}",
                    self.outbound_target - missing_connections,
                    self.outbound_target,
                    jobs.len(),
                    addr_iter.len(),
                );
            }
            for (res, net_addr) in (join_all(jobs).await).into_iter().zip(addrs_to_connect) {
                match res {
                    Ok(_) => {
                        self.address_manager.lock().mark_connection_success(net_addr);
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
            if missing_connections > self.outbound_target / 2 {
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

        let mut futures = Vec::with_capacity(active_inbound_len - self.inbound_limit);
        for peer in active_inbound.choose_multiple(&mut thread_rng(), active_inbound_len - self.inbound_limit) {
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
}

/// RAII helper for `ConnectionManager::add_endpoint_request`: removes
/// the host from `pending_registrations` on drop, including on early
/// returns and panics in the resolve / upsert phases.
struct PendingRegistrationGuard<'a> {
    pending: &'a ParkingLotMutex<HashSet<Arc<str>>>,
    host: Arc<str>,
}

impl Drop for PendingRegistrationGuard<'_> {
    fn drop(&mut self) {
        self.pending.lock().remove(&self.host);
    }
}

/// Yield from the periodic-ticker arm of `start_hostname_refresh_loop`.
/// When `ticker` is `None` (periodic refresh disabled with
/// `Duration::ZERO`), pend forever so only the dial-failure consumer
/// drives `refresh_hostnames` -- which is the documented contract on
/// `ConnectionManager::hostname_refresh_interval`.
async fn next_periodic_tick(ticker: &mut Option<tokio::time::Interval>) {
    match ticker {
        Some(t) => {
            t.tick().await;
        }
        None => std::future::pending::<()>().await,
    }
}
