use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use addressmanager::AddressManager;
use duration_string::DurationString;
use futures_util::future::join_all;
use itertools::Itertools;
use log::debug;
use p2p_lib::Peer;
use parking_lot::Mutex as ParkingLotMutex;
use rand::{seq::SliceRandom, thread_rng};
use tokio::{
    select,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex as TokioMutex,
    },
    time::interval,
};

pub struct ConnectionManager {
    p2p_adaptor: Arc<p2p_lib::Adaptor>,
    outbound_target: usize,
    inbound_limit: usize,
    amgr: Arc<ParkingLotMutex<AddressManager>>,
    connection_requests: TokioMutex<HashMap<SocketAddr, ConnectionRequest>>,
    force_next_iteration: UnboundedSender<()>,
    shutdown_signal: UnboundedSender<()>,
}

#[derive(Clone)]
struct ConnectionRequest {
    next_attempt: SystemTime,
    is_permanent: bool,
    attempts: u32,
}

impl ConnectionManager {
    pub fn new(
        p2p_adaptor: Arc<p2p_lib::Adaptor>,
        outbound_target: usize,
        inbound_limit: usize,
        amgr: Arc<ParkingLotMutex<AddressManager>>,
    ) -> Arc<Self> {
        let (tx, rx) = unbounded_channel::<()>();
        let (shutdown_signal_tx, shutdown_signal_rx) = unbounded_channel();
        let manager = Arc::new(Self {
            p2p_adaptor,
            outbound_target,
            inbound_limit,
            amgr,
            connection_requests: Default::default(),
            force_next_iteration: tx,
            shutdown_signal: shutdown_signal_tx,
        });
        manager.clone().start_event_loop(rx, shutdown_signal_rx);
        manager.force_next_iteration.send(()).unwrap();
        manager
    }

    fn start_event_loop(self: Arc<Self>, mut rx: UnboundedReceiver<()>, mut shutdown_signal_rx: UnboundedReceiver<()>) {
        let mut ticker = interval(Duration::from_secs(30));
        tokio::spawn(async move {
            loop {
                select! {
                    _ = rx.recv() => self.clone().handle_event().await,
                    _ = ticker.tick() => self.clone().handle_event().await,
                    _ = shutdown_signal_rx.recv() => break,
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

    pub async fn add_connection_request(&self, address: SocketAddr, is_permanent: bool) {
        // If the request already exists, it resets the attempts count and overrides the `is_permanent` setting.
        self.connection_requests
            .lock()
            .await
            .insert(address, ConnectionRequest { next_attempt: SystemTime::now(), is_permanent, attempts: 0 });
        self.force_next_iteration.send(()).unwrap(); // We force the next iteration of the connection loop.
    }

    pub async fn stop(&self) {
        self.shutdown_signal.send(()).unwrap();
    }

    async fn handle_connection_requests(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let mut requests = self.connection_requests.lock().await;
        let mut new_requests = HashMap::with_capacity(requests.len());
        for (address, request) in requests.iter() {
            let address = *address;
            let request = request.clone();
            let is_connected = peer_by_address.contains_key(&address);
            if is_connected && !request.is_permanent {
                continue;
            }

            if !is_connected && request.next_attempt <= SystemTime::now() {
                debug!("Connecting to a connection request to {}", address);
                if !self.p2p_adaptor.connect_peer(address.to_string()).await.is_none() {
                    debug!("Failed connecting to a connection request to {}", address);
                    if request.is_permanent {
                        const MAX_RETRY_DURATION: Duration = Duration::new(600, 0);
                        let retry_duration = max(Duration::new(30u64 * 2u64.pow(request.attempts), 0), MAX_RETRY_DURATION);
                        debug!("Will retry to connect to {} in {}", address, DurationString::from(retry_duration));
                        new_requests.insert(
                            address,
                            ConnectionRequest {
                                next_attempt: request.next_attempt + retry_duration,
                                attempts: request.attempts + 1,
                                is_permanent: true,
                            },
                        );
                    }
                }
            } else {
                new_requests.insert(address, request);
            }
        }

        *requests = new_requests;
    }

    async fn handle_outbound_connections(self: &Arc<Self>, peer_by_address: &HashMap<SocketAddr, Peer>) {
        let active_outbound: HashSet<addressmanager::NetAddress> =
            peer_by_address.values().filter(|peer| peer.is_outbound()).map(|peer| peer.net_address().into()).collect();
        let mut missing_connections = self.outbound_target - active_outbound.len();
        if missing_connections == 0 {
            return;
        }

        let addresses = self.amgr.lock().get_random_addresses(active_outbound);
        for net_addr in addresses {
            let socket_addr = SocketAddr::new(net_addr.ip, net_addr.port).to_string();
            debug!("Connecting to {}", &socket_addr);
            if !self.p2p_adaptor.connect_peer_with_retry_params(socket_addr.clone(), 1, Default::default()).await.is_some() {
                debug!("Failed connecting to {}", socket_addr);
                self.amgr.lock().mark_connection_failure(net_addr);
                missing_connections -= 1;
                if missing_connections == 0 {
                    break;
                }
            } else {
                self.amgr.lock().mark_connection_success(net_addr);
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
            futures.push(self.p2p_adaptor.terminate(peer.identity()));
        }
        join_all(futures).await;
    }
}
