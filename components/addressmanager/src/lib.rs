mod port_mapping_extender;
mod stores;

#[cfg(test)]
mod tests;

use kaspa_utils::connection::{AddressKey, LibP2pAddress, Libp2pId, PeerAddress};
extern crate self as address_manager;

use std::{
    collections::{HashMap, HashSet},
    iter,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use address_manager::port_mapping_extender::Extender;
use igd_next::{
    self as igd, AddAnyPortError, AddPortError, Gateway, GetExternalIpError, GetGenericPortMappingEntryError, SearchError,
    aio::tokio::Tokio,
};
use itertools::{
    Either::{Left, Right},
    Itertools,
};
use kaspa_consensus_core::config::Config;
use kaspa_core::{info, task::tick::TickService, time::unix_now, warn};
use kaspa_database::prelude::{CachePolicy, DB, StoreResultExt};
use kaspa_utils::networking::{IpAddress, PrefixBucket};
use local_ip_address::list_afinet_netifas;
use parking_lot::Mutex;
use stores::banned_address_store::{BannedAddressesStore, BannedAddressesStoreReader, ConnectionBanTimestamp, DbBannedAddressesStore};
use thiserror::Error;

pub use stores::{NetAddress, address_store::DbAddressesStore};

pub const MAX_ADDRESSES: usize = 4096;
const MAX_CONNECTION_FAILED_COUNT: u64 = 3;

const UPNP_DEADLINE_SEC: u64 = 2 * 60;
const UPNP_EXTEND_PERIOD: u64 = UPNP_DEADLINE_SEC / 2;

/// The name used as description when registering the UPnP service
pub(crate) const UPNP_REGISTRATION_NAME: &str = "rusty-kaspa";

struct ExtendHelper {
    gateway: Gateway,
    local_addr: SocketAddr,
    external_port: u16,
}

#[derive(Error, Debug)]
pub enum UpnpError {
    #[error(transparent)]
    AddPortError(#[from] AddPortError),
    #[error(transparent)]
    AddAnyPortError(#[from] AddAnyPortError),
    #[error(transparent)]
    SearchError(#[from] SearchError),
    #[error(transparent)]
    GetExternalIpError(#[from] GetExternalIpError),
}

#[derive(Error, Debug)]
pub enum LocalAdvertisementError<E> {
    #[error("local address has no libp2p metadata")]
    MissingMetadata,
    #[error("local address nonce exhausted")]
    NonceExhausted,
    #[error("local address signing failed: {0}")]
    Signing(#[source] E),
}

pub struct AddressManager {
    banned_address_store: DbBannedAddressesStore,
    address_store: address_store_with_cache::Store,
    config: Arc<Config>,
    local_net_addresses: Vec<NetAddress>,
    /// This node's authenticated Libp2p identity, set after the endpoint starts.
    local_libp2p_id: Option<Libp2pId>,
}

impl AddressManager {
    pub fn new(config: Arc<Config>, db: Arc<DB>, tick_service: Arc<TickService>) -> (Arc<Mutex<Self>>, Option<Extender>) {
        let mut instance = Self {
            banned_address_store: DbBannedAddressesStore::new(db.clone(), CachePolicy::Count(MAX_ADDRESSES)),
            address_store: address_store_with_cache::new(db),
            local_net_addresses: Vec::new(),
            local_libp2p_id: None,
            config,
        };

        let extender = instance.init_local_addresses(tick_service);

        (Arc::new(Mutex::new(instance)), extender)
    }

    fn init_local_addresses(&mut self, tick_service: Arc<TickService>) -> Option<Extender> {
        self.local_net_addresses = self.local_addresses().collect();

        let extender = if self.local_net_addresses.is_empty() && !self.config.disable_upnp {
            let (net_address, ExtendHelper { gateway, local_addr, external_port }) = match self.upnp() {
                Err(err) => {
                    warn!("[UPnP] Error adding port mapping: {err}");
                    return None;
                }
                Ok(None) => return None,
                Ok(Some((net_address, extend_helper))) => (net_address, extend_helper),
            };
            self.local_net_addresses.push(net_address);

            let gateway: igd_next::aio::Gateway<Tokio> = igd_next::aio::Gateway {
                addr: gateway.addr,
                root_url: gateway.root_url,
                control_url: gateway.control_url,
                control_schema_url: gateway.control_schema_url,
                control_schema: gateway.control_schema,
                provider: Tokio,
            };
            Some(Extender::new(
                tick_service,
                Duration::from_secs(UPNP_EXTEND_PERIOD),
                UPNP_DEADLINE_SEC,
                gateway,
                external_port,
                local_addr,
            ))
        } else {
            None
        };

        self.local_net_addresses.iter().for_each(|net_addr| {
            info!("Local TCP listener address {} available for advertisement", net_addr);
        });
        extender
    }

    fn local_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
        match self.config.externalip {
            // An external IP was passed, we will try to bind that if it's valid
            Some(local_net_address) if self.is_local_address_advertisable(local_net_address.ip) => {
                info!("Using advertised TCP listener address {}", local_net_address);
                return Left(iter::once(local_net_address));
            }
            Some(local_net_address) => {
                info!("External address is not publicly routable {}", local_net_address);
            }
            None => {}
        };

        Right(self.routable_addresses_from_net_interfaces())
    }

    fn routable_addresses_from_net_interfaces(&self) -> impl Iterator<Item = NetAddress> + '_ {
        // check whatever was passed as listen address (if routable)
        // otherwise(listen_address === 0.0.0.0) check all interfaces
        let listen_address = self.config.p2p_listen_address.normalize(self.config.default_p2p_port());
        if self.is_local_address_advertisable(listen_address.ip) {
            info!("Local TCP listener address found: {}", listen_address);
            Left(Left(iter::once(listen_address)))
        } else if listen_address.ip.is_unspecified() {
            let network_interfaces = list_afinet_netifas();
            let Ok(network_interfaces) = network_interfaces else {
                warn!("Error getting network interfaces: {:?}", network_interfaces);
                return Left(Right(iter::empty()));
            };
            // TODO: Add Check IPv4 or IPv6 match from Go code
            Right(
                network_interfaces
                    .into_iter()
                    .map(|(_, ip)| IpAddress::from(ip))
                    .filter(|&ip| self.is_local_address_advertisable(ip))
                    .map(move |ip| {
                        info!("Local TCP listener interface found: {}", ip);
                        NetAddress::new(ip, listen_address.port)
                    }),
            )
        } else {
            Left(Right(iter::empty()))
        }
    }

    fn is_local_address_advertisable(&self, ip: IpAddress) -> bool {
        ip.is_publicly_routable()
            || (matches!(
                self.config.net.network_type,
                kaspa_consensus_core::network::NetworkType::Simnet | kaspa_consensus_core::network::NetworkType::Devnet
            ) && !ip.is_unspecified()
                && !ip.0.is_multicast())
    }

    fn upnp(&self) -> Result<Option<(NetAddress, ExtendHelper)>, UpnpError> {
        info!("[UPnP] Attempting to register upnp... (to disable run the node with --disable-upnp)");
        let gateway = igd::search_gateway(Default::default())?;
        let ip = IpAddress::new(gateway.get_external_ip()?);
        if !ip.is_publicly_routable() {
            info!("[UPnP] Non-publicly routable external ip from gateway using upnp {} not added to store", ip);
            return Ok(None);
        }
        info!("[UPnP] Got external ip from gateway using upnp: {ip}");

        let normalized_p2p_listen_address = self.config.p2p_listen_address.normalize(self.config.default_p2p_port());
        let local_addr = if normalized_p2p_listen_address.ip.is_unspecified() {
            SocketAddr::new(local_ip_address::local_ip().unwrap(), normalized_p2p_listen_address.port)
        } else {
            normalized_p2p_listen_address.into()
        };

        // If an operator runs a node and specifies a non-standard local port, it implies that they also wish to use a non-standard public address. The variable 'desired_external_port' is set to the port number from the normalized peer-to-peer listening address.
        let desired_external_port = normalized_p2p_listen_address.port;
        // This loop checks for existing port mappings in the UPnP-enabled gateway.
        //
        // The goal of this loop is to identify if the desired external port (`desired_external_port`) is
        // already mapped to any device inside the local network. This is crucial because, in
        // certain scenarios, gateways might not throw the `PortInUse` error but rather might
        // silently remap the external port when there's a conflict. By iterating through the
        // current mappings, we can make an informed decision about whether to attempt using
        // the default port or request a new random one.
        //
        // The loop goes through all existing port mappings one-by-one:
        // - If a mapping is found that uses the desired external port, the loop breaks with `already_in_use` set to true.
        // - If the index is not valid (i.e., we've iterated through all the mappings), the loop breaks with `already_in_use` set to false.
        // - Any other errors during fetching of port mappings are handled accordingly, but the end result is to exit the loop with the `already_in_use` flag set appropriately.
        let mut already_in_use = false;
        for i in 0.. {
            match gateway.get_generic_port_mapping_entry(i) {
                Ok(entry) => {
                    if entry.enabled && entry.external_port == desired_external_port {
                        info!(
                            "[UPnP] Found existing mapping that uses the same external port. Description: {}, external port: {}, internal port: {}, client: {}, lease duration: {}",
                            entry.port_mapping_description,
                            entry.external_port,
                            entry.internal_port,
                            entry.internal_client,
                            entry.lease_duration
                        );
                        already_in_use = true;
                        break;
                    }
                }
                Err(GetGenericPortMappingEntryError::ActionNotAuthorized) => {
                    continue;
                }
                Err(GetGenericPortMappingEntryError::RequestError(err)) => {
                    warn!("[UPnP] request existing port mapping err: {:?}", err);
                    break;
                }
                Err(GetGenericPortMappingEntryError::SpecifiedArrayIndexInvalid) => break,
            }
        }
        if already_in_use {
            let port =
                gateway.add_any_port(igd::PortMappingProtocol::TCP, local_addr, UPNP_DEADLINE_SEC as u32, UPNP_REGISTRATION_NAME)?;
            info!("[UPnP] Added port mapping to random external port: {ip}:{port}");
            return Ok(Some((NetAddress { ip, port }, ExtendHelper { gateway, local_addr, external_port: port })));
        }

        match gateway.add_port(
            igd::PortMappingProtocol::TCP,
            desired_external_port,
            local_addr,
            UPNP_DEADLINE_SEC as u32,
            UPNP_REGISTRATION_NAME,
        ) {
            Ok(_) => {
                info!("[UPnP] Added port mapping to default external port: {ip}:{desired_external_port}");
                Ok(Some((
                    NetAddress { ip, port: desired_external_port },
                    ExtendHelper { gateway, local_addr, external_port: desired_external_port },
                )))
            }
            Err(AddPortError::PortInUse) => {
                let port = gateway.add_any_port(
                    igd::PortMappingProtocol::TCP,
                    local_addr,
                    UPNP_DEADLINE_SEC as u32,
                    UPNP_REGISTRATION_NAME,
                )?;
                info!("[UPnP] Added port mapping to random external port: {ip}:{port}");
                Ok(Some((NetAddress { ip, port }, ExtendHelper { gateway, local_addr, external_port: port })))
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn best_local_address(&self) -> Option<NetAddress> {
        // Simnet/devnet can include loopback alongside a usable interface.
        // Prefer the interface peers can reach, retaining loopback-only setups.
        self.local_net_addresses.iter().find(|address| !address.ip.is_loopback()).or_else(|| self.local_net_addresses.first()).copied()
    }

    /// Reuses an unchanged local record, or signs its next nonce and publishes it in memory.
    pub fn sign_local_address<E>(
        &mut self,
        mut address: PeerAddress,
        sign: impl FnOnce(&PeerAddress) -> Result<Vec<u8>, E>,
    ) -> Result<PeerAddress, LocalAdvertisementError<E>> {
        let previous = self.address_store.local_advertisement();
        let metadata = address.libp2p.as_mut().ok_or(LocalAdvertisementError::MissingMetadata)?;
        if let Some(previous) = &previous {
            metadata.nonce = previous.nonce().unwrap_or_default();
            metadata.signature = previous.signature().unwrap_or_default().to_vec();
        }
        if previous.as_ref() == Some(&address) {
            return Ok(address);
        }
        let nonce = previous
            .as_ref()
            .and_then(PeerAddress::nonce)
            .unwrap_or_default()
            .checked_add(1)
            .ok_or(LocalAdvertisementError::NonceExhausted)?;
        address.libp2p.as_mut().unwrap().nonce = nonce;
        let signature = sign(&address).map_err(LocalAdvertisementError::Signing)?;
        address.libp2p.as_mut().unwrap().signature = signature;
        self.address_store.set_local_advertisement(address.clone());
        Ok(address)
    }

    /// Whether this run has published a signed address, independently of the TCP fallback.
    pub fn local_advertisement_initialized(&self) -> bool {
        self.address_store.local_advertisement().is_some()
    }

    /// Returns this run's signed advertisement, or the best local TCP address until initialization.
    pub fn local_advertisement(&self) -> Option<PeerAddress> {
        self.address_store.local_advertisement().or_else(|| self.best_local_address().map(PeerAddress::from))
    }

    /// Updates only the relay route after initial publication. Renewing the same reservation is a no-op.
    pub fn update_local_relay<E>(
        &mut self,
        relay: Option<LibP2pAddress>,
        sign: impl FnOnce(&PeerAddress) -> Result<Vec<u8>, E>,
    ) -> Result<(), LocalAdvertisementError<E>> {
        if let Some(mut address) = self.address_store.local_advertisement()
            && address.relay_address != relay
        {
            address.relay_address = relay;
            self.sign_local_address(address, sign)?;
        }
        Ok(())
    }

    pub fn set_local_libp2p_id(&mut self, id: Libp2pId) {
        self.local_libp2p_id = Some(id);
    }

    /// Accepts fresh records without checking signatures; verifies competing records before replacement.
    pub fn add_address(&mut self, address: PeerAddress) {
        if address.ip.is_unspecified()
            || address.ip.0.is_multicast()
            || address.endpoint_id().is_some_and(|id| Some(id) == self.local_libp2p_id)
        {
            return;
        }
        if address.libp2p.is_none() && address.ip.is_loopback() {
            return;
        }
        let key = address.key();
        let mut signature_verified = false;
        if let Some(existing) = self.address_store.get(&key) {
            let network = self.config.network_name();
            if !address.verify(&network) {
                return;
            }
            signature_verified = true;
            // If the stored signature is invalid, a correctly signed update may replace it regardless of nonce.
            if address.nonce().unwrap_or_default() <= existing.nonce {
                if existing.signature_verified {
                    return;
                }
                if existing.address(key).verify(&network) {
                    let mut existing = existing.clone();
                    existing.signature_verified = true;
                    self.address_store.set(key, existing);
                    return;
                }
            }
        }
        self.address_store.upsert_address(address, signature_verified);
    }

    pub fn iterator(&self) -> impl Iterator<Item = PeerAddress> + '_ {
        self.address_store.iterator()
    }

    pub fn address(&self, key: AddressKey) -> Option<PeerAddress> {
        self.address_store.get(&key).map(|entry| entry.address(key))
    }

    /// Records relay registration failures locally without changing the signed advertisement.
    pub fn record_relay_result(&mut self, address: &PeerAddress, success: bool) {
        let key = address.key();
        let Some(mut entry) = self.address_store.get(&key).cloned() else {
            return;
        };
        if entry.relay_port != address.relay_port() {
            return;
        }
        entry.consecutive_relay_failures = if success { 0 } else { entry.consecutive_relay_failures.saturating_add(1) };
        self.address_store.set(key, entry);
    }

    /// Weight = `64^(MAX_CONNECTION_FAILED_COUNT + 1 - failures) / prefix_bucket_size`.
    pub fn iterate_prioritized_random_relays(&mut self) -> impl ExactSizeIterator<Item = (PeerAddress, u64)> + 'static {
        let self_id = self.local_libp2p_id;
        let mut candidates = self
            .address_store
            .entries()
            .filter_map(|(key, entry)| {
                let address = entry.address(*key);
                (address.relay_port().is_some_and(|port| port != 0) && address.endpoint_id() != self_id)
                    .then_some((address, entry.consecutive_relay_failures))
            })
            .collect::<Vec<_>>();
        candidates.retain(|(address, _)| !self.is_banned(address.ip));
        let mut prefix_counter: HashMap<PrefixBucket, usize> = HashMap::new();
        let mut weights = candidates
            .iter()
            .map(|(address, failures)| {
                #[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(COUNTER)")]
                {
                    *prefix_counter.entry(address.prefix_bucket()).or_default() += 1;
                }
                // Keep the same weight calculation as iterate_prioritized_random_addresses for consistency.
                #[allow(
                    clippy::arithmetic_side_effects,
                    reason = "The exponent uses f64 arithmetic; these u64 inputs and their sum or difference remain finite."
                )]
                {
                    64f64.powf(MAX_CONNECTION_FAILED_COUNT as f64 + 1.0f64 - *failures as f64)
                }
            })
            .collect::<Vec<_>>();
        // Normalize over IP prefixes, just as for ordinary peer addresses.
        for (weight, (address, _)) in weights.iter_mut().zip(&candidates) {
            *weight /= prefix_counter[&address.prefix_bucket()] as f64;
        }
        address_store_with_cache::RandomWeightedIterator::new(weights, candidates)
    }

    pub fn mark_connection_failure(&mut self, address: PeerAddress) {
        self.record_connection_result(address, false);
    }

    pub fn mark_connection_success(&mut self, address: PeerAddress) {
        self.record_connection_result(address, true);
    }

    fn record_connection_result(&mut self, address: PeerAddress, success: bool) {
        let key = address.key();
        let Some(mut entry) = self.address_store.get(&key).cloned() else {
            return;
        };
        entry.connection_failed_count = if success { 0 } else { entry.connection_failed_count.saturating_add(1) };
        if entry.connection_failed_count > MAX_CONNECTION_FAILED_COUNT {
            self.address_store.remove_by_key(key);
        } else {
            self.address_store.set(key, entry);
        }
    }

    pub fn iterate_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
        self.address_store.entries().map(|(key, _)| key.0).unique()
    }

    pub fn iterate_prioritized_random_addresses(
        &self,
        tcp_exceptions: HashSet<NetAddress>,
        libp2p_exceptions: HashSet<Libp2pId>,
    ) -> impl ExactSizeIterator<Item = PeerAddress> + 'static {
        self.address_store.iterate_prioritized_random_addresses(tcp_exceptions, libp2p_exceptions)
    }

    pub fn ban(&mut self, ip: IpAddress) {
        self.banned_address_store.set(ip.into(), ConnectionBanTimestamp(unix_now())).unwrap();
        self.address_store.remove_by_ip(ip.into());
    }

    pub fn unban(&mut self, ip: IpAddress) {
        self.banned_address_store.remove(ip.into()).unwrap();
    }

    pub fn is_banned(&mut self, ip: IpAddress) -> bool {
        const MAX_BANNED_TIME: u64 = 24 * 60 * 60 * 1000;
        match self.banned_address_store.get(ip.into()).optional().unwrap() {
            Some(timestamp) => {
                if unix_now().saturating_sub(timestamp.0) > MAX_BANNED_TIME {
                    self.unban(ip);
                    false
                } else {
                    true
                }
            }
            None => false,
        }
    }

    pub fn get_all_addresses(&self) -> Vec<NetAddress> {
        self.iterate_addresses().collect_vec()
    }

    pub fn get_all_banned_addresses(&self) -> Vec<IpAddress> {
        self.banned_address_store.iterator().map(|x| IpAddress::from(x.unwrap().0)).collect_vec()
    }
}

mod address_store_with_cache {
    use crate::{
        MAX_ADDRESSES, MAX_CONNECTION_FAILED_COUNT,
        stores::address_store::{DbAddressesStore, Entry},
    };
    use itertools::Itertools;
    use kaspa_database::prelude::{CachePolicy, DB};
    use kaspa_utils::{
        connection::{AddressKey, Libp2pId, PeerAddress},
        networking::{NetAddress, PrefixBucket},
    };
    use rand::{
        distributions::{WeightedError, WeightedIndex},
        prelude::Distribution,
    };
    use std::{
        collections::{HashMap, HashSet},
        net::IpAddr,
        sync::Arc,
    };

    pub struct Store {
        db_store: DbAddressesStore,
        addresses: HashMap<AddressKey, Entry>,
        /// Signed local address for this run; never written to the address database.
        local_advertisement: Option<PeerAddress>,
    }

    impl Store {
        fn new(db: Arc<DB>) -> Self {
            // The in-memory map already caches all address entries.
            let db_store = DbAddressesStore::new(db, CachePolicy::Empty);
            let addresses = db_store.iterator().map(|row| row.expect("address database entry")).collect();
            let mut store = Self { db_store, addresses, local_advertisement: None };
            store.keep_limit();
            store
        }

        pub(super) fn get(&self, key: &AddressKey) -> Option<&Entry> {
            self.addresses.get(key)
        }

        pub(super) fn entries(&self) -> impl Iterator<Item = (&AddressKey, &Entry)> {
            self.addresses.iter()
        }

        pub(super) fn local_advertisement(&self) -> Option<PeerAddress> {
            self.local_advertisement.clone()
        }

        pub(super) fn set_local_advertisement(&mut self, address: PeerAddress) {
            self.local_advertisement = Some(address);
        }

        pub(super) fn set(&mut self, key: AddressKey, entry: Entry) {
            self.db_store.set(&key, &entry).unwrap();
            self.addresses.insert(key, entry);
            self.keep_limit();
        }

        pub fn upsert_address(&mut self, address: PeerAddress, signature_verified: bool) {
            let key = address.key();
            let mut entry = match self.addresses.get(&key) {
                Some(existing) => existing.clone(),
                // Start untried addresses at 1 so successful peers and relays (count 0) get higher selection priority.
                None => Entry { connection_failed_count: 1, consecutive_relay_failures: 1, ..Default::default() },
            };
            entry.update(&address, signature_verified);
            self.set(key, entry);
        }

        fn keep_limit(&mut self) {
            let excess = self.addresses.len().saturating_sub(MAX_ADDRESSES);
            if excess == 0 {
                return;
            }
            let keys = self
                .addresses
                .iter()
                .k_largest_by_key(excess, |(_, entry)| entry.connection_failed_count)
                .map(|(key, _)| *key)
                .collect::<Vec<_>>();
            for key in keys {
                self.remove_by_key(key);
            }
        }

        pub(super) fn remove_by_key(&mut self, key: AddressKey) {
            self.addresses.remove(&key);
            self.db_store.remove(&key).unwrap();
        }

        pub fn iterator(&self) -> impl Iterator<Item = PeerAddress> + '_ {
            self.addresses.iter().map(|(key, entry)| entry.address(*key))
        }

        /// This iterator functions as the node's ip routing selection algo.
        /// It first adjusts in respect to the number of connection failures of each ip address,
        /// whereby each connection failure (up to [`MAX_CONNECTION_FAILED_COUNT`]) reduces an ip's selection weight by a factor of 64,
        /// Afterwards the weights are normalized uniformly over the ip's [`PrefixBucket`] size.
        ///
        /// This ensures a distributed selection across the global network, while respecting
        /// weight reductions due to ip connection failures.
        ///
        /// The exact weight formula for any given ip, is as follows:
        ///```ignore
        ///         ip_weight = (64 ^ (x - y)) / n
        ///
        ///             whereby:
        ///                 x: max allowed connection failures.
        ///                 y: connection failures of the ip.
        ///                 n: number of ips with the same prefix bytes.
        ///```
        pub fn iterate_prioritized_random_addresses(
            &self,
            tcp_exceptions: HashSet<NetAddress>,
            libp2p_exceptions: HashSet<Libp2pId>,
        ) -> impl ExactSizeIterator<Item = PeerAddress> + 'static {
            let mut prefix_counter: HashMap<PrefixBucket, usize> = HashMap::new();
            let (mut weights, filtered_addresses): (Vec<f64>, Vec<PeerAddress>) = self
                .addresses
                .iter()
                .filter(|(key, _)| !tcp_exceptions.contains(&key.0) && key.1.is_none_or(|id| !libp2p_exceptions.contains(&id)))
                .map(|(key, e)| {
                    let address = e.address(*key);
                    let count = prefix_counter.entry(address.prefix_bucket()).or_insert(0);
                    #[allow(clippy::arithmetic_side_effects, reason = "See below")]
                    {
                        *count += 1; // ARITH-SAFETY(COUNTER)
                        (64f64.powf((MAX_CONNECTION_FAILED_COUNT + 1 - e.connection_failed_count) as f64), address) // It's guaranteed that `e.connection_failed_count <= MAX_CONNECTION_FAILED_COUNT`.
                    }
                })
                .unzip();

            // Divide weights by size of bucket of the prefix bytes, to partially uniform the distribution over prefix buckets.
            for (i, address) in filtered_addresses.iter().enumerate() {
                *weights.get_mut(i).unwrap() /= *prefix_counter.get(&address.prefix_bucket()).unwrap() as f64;
            }

            RandomWeightedIterator::new(weights, filtered_addresses)
        }

        pub fn remove_by_ip(&mut self, ip: IpAddr) {
            let ip = ip.to_canonical();
            let keys = self.db_store.iterator_by_ip(ip).map(|row| row.expect("address database entry").0).collect_vec();
            for key in keys {
                self.remove_by_key(key);
            }
        }
    }

    pub fn new(db: Arc<DB>) -> Store {
        Store::new(db)
    }

    pub struct RandomWeightedIterator<T> {
        weighted_index: Option<WeightedIndex<f64>>,
        remaining: usize,
        addresses: Vec<T>,
    }

    impl<T> RandomWeightedIterator<T> {
        pub fn new(weights: Vec<f64>, addresses: Vec<T>) -> Self {
            assert_eq!(weights.len(), addresses.len());
            let remaining = weights.iter().filter(|&&w| w > 0.0).count();
            let weighted_index = match WeightedIndex::new(weights) {
                Ok(index) => Some(index),
                Err(WeightedError::NoItem) => None,
                Err(WeightedError::AllWeightsZero) => None,
                Err(e) => panic!("{e}"),
            };
            Self { weighted_index, remaining, addresses }
        }
    }

    impl<T: Clone> Iterator for RandomWeightedIterator<T> {
        type Item = T;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(weighted_index) = self.weighted_index.as_mut() {
                let i = weighted_index.sample(&mut rand::thread_rng());
                // Zero the selected address entry
                match weighted_index.update_weights(&[(i, &0f64)]) {
                    Ok(_) => {}
                    Err(WeightedError::AllWeightsZero) => self.weighted_index = None,
                    Err(e) => panic!("{e}"),
                }

                #[allow(
                    clippy::arithmetic_side_effects,
                    reason = "It's guaranteed that once `remaining = 0` then `weighted_index = None`, and then this code path will not be reached."
                )]
                {
                    self.remaining -= 1;
                }

                if self.remaining == 0 {
                    self.weighted_index = None;
                }
                Some(self.addresses[i].clone())
            } else {
                None
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.remaining, Some(self.remaining))
        }
    }

    impl<T: Clone> ExactSizeIterator for RandomWeightedIterator<T> {}

    #[cfg(test)]
    mod tests {
        use std::str::FromStr;

        use super::*;
        use address_manager::AddressManager;
        use kaspa_consensus_core::config::{Config, params::SIMNET_PARAMS};
        use kaspa_core::task::tick::TickService;
        use kaspa_database::create_temp_db;
        use kaspa_database::prelude::ConnBuilder;
        use kaspa_utils::networking::{IpAddress, NetAddress};
        use rv::{dist::Uniform, misc::ks_test as one_way_ks_test, traits::Cdf};
        use std::net::{IpAddr, Ipv6Addr};

        #[test]
        fn test_weighted_iterator() {
            let address = NetAddress::new(IpAddr::V6(Ipv6Addr::LOCALHOST).into(), 1);
            let iter = RandomWeightedIterator::<PeerAddress>::new(vec![0.2, 0.3, 0.0], vec![address.into(); 3]);
            assert_eq!(iter.len(), 2);
            assert_eq!(iter.count(), 2);

            let iter = RandomWeightedIterator::<PeerAddress>::new(vec![], vec![]);
            assert_eq!(iter.len(), 0);
            assert_eq!(iter.count(), 0);
        }

        // This test is indeterminate, so it is ignored by default.
        // Every developer that changes the logic of the address manager should run this test locally before sending a PR.
        // TODO: Maybe change statistical parameters to reduce the failure rate?
        #[test]
        #[ignore]
        fn test_network_distribution_weighting() {
            kaspa_core::log::try_init_logger("info");

            // Variables to initialize ip generation with.
            let largest_bucket: u16 = 2048;
            let bucket_reduction_ratio: f64 = 2.;

            // Assert that initial distribution is skewed, and hence not uniform from the outset.
            assert!(bucket_reduction_ratio >= 1.25);

            let db = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let config = Config::new(SIMNET_PARAMS);
            let (am, _) = AddressManager::new(Arc::new(config), db.1, Arc::new(TickService::default()));

            let mut am_guard = am.lock();

            let mut num_of_buckets = 0;
            let mut num_of_addresses = 0;
            let mut current_bucket_size = largest_bucket;

            for current_prefix_bytes in 0..u16::MAX {
                num_of_buckets += 1;
                for current_suffix_bytes in 0..current_bucket_size {
                    let current_ip_bytes =
                        [current_prefix_bytes.to_be_bytes(), current_suffix_bytes.to_be_bytes()].concat().to_owned();
                    am_guard.add_address(PeerAddress::from(NetAddress::new(
                        IpAddress::from_str(&format!(
                            "{0}.{1}.{2}.{3}",
                            current_ip_bytes[0], current_ip_bytes[1], current_ip_bytes[2], current_ip_bytes[3]
                        ))
                        .unwrap(),
                        16111,
                    )));
                    num_of_addresses += 1;
                }

                let last_bucket_size = current_bucket_size;
                current_bucket_size = ((current_bucket_size as f64) * (1.0 / bucket_reduction_ratio)).round() as u16;

                if current_bucket_size == last_bucket_size || current_bucket_size == 0 || current_prefix_bytes == u16::MAX {
                    // Address generation exhausted - exit loop
                    break;
                }
            }
            drop(am_guard);

            // Assert sample size is large enough.
            assert!(1024 <= num_of_addresses);
            // Assert we don't over-generate the address manager's limit.
            assert!(num_of_addresses <= MAX_ADDRESSES);
            // Assert that the test has enough buckets to sample from
            assert!(num_of_buckets >= 12);

            // Run multiple Kolmogorov–Smirnov tests to offset random noise of the random weighted iterator
            let num_of_trials = 2048; // Number of trials to run the test, chosen to reduce random noise.
            let mut cul_p = 0.;
            // The target uniform distribution
            let target_uniform_dist = Uniform::new(1.0, num_of_buckets as f64).unwrap();
            let uniform_cdf = |x: f64| target_uniform_dist.cdf(&x);
            for _ in 0..num_of_trials {
                // The weight sampled expected uniform distribution
                let prioritized_address_distribution = am
                    .lock()
                    .iterate_prioritized_random_addresses(HashSet::new(), HashSet::new())
                    .take(num_of_buckets)
                    .map(|addr| addr.prefix_bucket().as_u64() as f64)
                    .collect_vec();
                cul_p += one_way_ks_test(prioritized_address_distribution.as_slice(), uniform_cdf).1;
            }

            // Normalize and adjust p to test for uniformity, over average of all trials.
            // we do this to reduce the effect of random noise failing this test.
            let adjusted_p = ((cul_p / num_of_trials as f64) - 0.5).abs();
            // Define the significance threshold.
            let significance = 0.10;

            // Display and assert the result
            kaspa_core::info!(
                "Kolmogorov–Smirnov test result for weighted network distribution uniformity: p = {0:.4} (p < {1})",
                adjusted_p,
                significance
            );
            assert!(adjusted_p <= significance);
        }
    }
}
