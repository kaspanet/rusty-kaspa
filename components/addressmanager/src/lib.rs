mod stores;

extern crate self as address_manager;

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use database::prelude::{StoreResultExtensions, DB};
use kaspa_core::time::unix_now;
use parking_lot::Mutex;
use stores::{
    banned_address_store::{BannedAddressesStore, BannedAddressesStoreReader, ConnectionBanTimestamp, DbBannedAddressesStore},
    not_banned_address_store::ConnectionFailureCount,
};

const MAX_ADDRESSES: usize = 4096;
const MAX_CONNECTION_FAILED_COUNT: u64 = 3;

pub struct AddressManager {
    banned_address_store: DbBannedAddressesStore,
    not_banned_address_store: not_banned_address_store_with_cache::Store,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub struct NetAddress {
    pub ip: Ipv6Addr,
    pub port: u16,
}

impl NetAddress {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self {
            ip: match ip {
                IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                IpAddr::V6(ip) => ip,
            },
            port,
        }
    }
}

impl From<SocketAddr> for NetAddress {
    fn from(value: SocketAddr) -> Self {
        Self::new(value.ip(), value.port())
    }
}

impl AddressManager {
    pub fn new(db: Arc<DB>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            banned_address_store: DbBannedAddressesStore::new(db.clone(), MAX_ADDRESSES as u64),
            not_banned_address_store: not_banned_address_store_with_cache::new(db),
        }))
    }

    pub fn add_address(&mut self, address: NetAddress) {
        // TODO: Don't add non routable addresses

        if self.not_banned_address_store.has(address) {
            return;
        }

        // We mark `connection_failed_count` as 0 only after first success
        self.not_banned_address_store.set(address, ConnectionFailureCount(1));
    }

    pub fn mark_connection_failure(&mut self, address: NetAddress) {
        if !self.not_banned_address_store.has(address) {
            return;
        }

        let new_count = self.not_banned_address_store.get(address).0 + 1;
        if new_count > MAX_CONNECTION_FAILED_COUNT {
            self.not_banned_address_store.remove(address);
        } else {
            self.not_banned_address_store.set(address, ConnectionFailureCount(new_count));
        }
    }

    pub fn mark_connection_success(&mut self, address: NetAddress) {
        if !self.not_banned_address_store.has(address) {
            return;
        }

        self.not_banned_address_store.set(address, ConnectionFailureCount(0));
    }

    pub fn get_all_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
        self.not_banned_address_store.get_all_addresses()
    }

    pub fn get_random_addresses(&self, exceptions: HashSet<NetAddress>) -> Vec<NetAddress> {
        self.not_banned_address_store.get_random_addresses(exceptions)
    }

    pub fn ban(&mut self, ip: Ipv6Addr) {
        self.banned_address_store.set(ip, ConnectionBanTimestamp(unix_now())).unwrap();
        self.not_banned_address_store.remove_by_ip(ip);
    }

    pub fn unban(&mut self, ip: Ipv6Addr) {
        self.banned_address_store.remove(ip).unwrap();
    }

    pub fn is_banned(&mut self, ip: Ipv6Addr) -> bool {
        const MAX_BANNED_TIME: u64 = 24 * 60 * 60 * 1000;
        match self.banned_address_store.get(ip).unwrap_option() {
            Some(timestamp) => {
                if unix_now() - timestamp.0 > MAX_BANNED_TIME {
                    self.unban(ip);
                    false
                } else {
                    true
                }
            }
            None => false,
        }
    }
}

mod not_banned_address_store_with_cache {
    // Since we need operations such as iterating all addresses, count, etc, we keep an easy to use copy of the database addresses.
    // We don't expect it to be expensive since we limit the number of saved addresses.
    use std::{
        collections::{HashMap, HashSet},
        net::Ipv6Addr,
        sync::Arc,
    };

    use database::prelude::DB;
    use itertools::Itertools;
    use rand::{distributions::WeightedIndex, prelude::Distribution};

    use crate::{
        stores::not_banned_address_store::{ConnectionFailureCount, DbNotBannedAddressesStore, NotBannedAddressesStore},
        NetAddress, MAX_ADDRESSES, MAX_CONNECTION_FAILED_COUNT,
    };

    pub struct Store {
        db_store: DbNotBannedAddressesStore,
        addresses: HashMap<NetAddress, ConnectionFailureCount>,
    }

    impl Store {
        fn new(db: Arc<DB>) -> Self {
            let db_store = DbNotBannedAddressesStore::new(db, 0);
            let mut addresses = HashMap::new();
            for ((ip, port), connection_failed_count) in db_store.iterator().map(|res| res.unwrap()) {
                addresses.insert(NetAddress { ip, port }, connection_failed_count);
            }

            Self { db_store, addresses }
        }

        pub fn has(&mut self, address: NetAddress) -> bool {
            self.addresses.contains_key(&address)
        }

        pub fn set(&mut self, address: NetAddress, connection_failed_count: ConnectionFailureCount) {
            self.db_store.set(address.ip, address.port, connection_failed_count).unwrap();
            self.addresses.insert(NetAddress::new(address.ip.into(), address.port), connection_failed_count);
            self.keep_limit();
        }

        fn keep_limit(&mut self) {
            while self.addresses.len() > MAX_ADDRESSES {
                let to_remove = self.addresses.iter().max_by(|a, b| (a.1).0.cmp(&(b.1).0)).unwrap();
                self.remove(*to_remove.0);
            }
        }

        pub fn get(&self, address: NetAddress) -> ConnectionFailureCount {
            *self.addresses.get(&address).unwrap()
        }

        pub fn remove(&mut self, address: NetAddress) {
            self.addresses.remove(&address);
            self.db_store.remove(address.ip, address.port).unwrap()
        }

        pub fn get_all_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
            self.addresses.keys().copied()
        }

        pub fn get_random_addresses(&self, exceptions: HashSet<NetAddress>) -> Vec<NetAddress> {
            let addresses = self.addresses.iter().filter(|(addr, _)| !exceptions.contains(addr)).collect_vec();
            let mut weights = addresses
                .iter()
                .map(|(_, failure_count)| 64f64.powf((MAX_CONNECTION_FAILED_COUNT + 1 - failure_count.0) as f64))
                .collect_vec();

            (0..addresses.len())
                .map(|_| {
                    let dist = WeightedIndex::new(&weights).unwrap();
                    let i = dist.sample(&mut rand::thread_rng());
                    let addr = addresses[i];
                    weights[i] = 0f64;
                    *addr.0
                })
                .collect_vec()
        }

        pub fn remove_by_ip(&mut self, ip: Ipv6Addr) {
            for addr in self.addresses.keys().filter(|addr| addr.ip == ip).copied().collect_vec() {
                self.remove(addr);
            }
        }
    }

    pub fn new(db: Arc<DB>) -> Store {
        Store::new(db)
    }
}
