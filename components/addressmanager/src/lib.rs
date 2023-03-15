mod stores;

extern crate self as address_manager;

use std::{collections::HashSet, net::IpAddr, sync::Arc};

use database::prelude::{StoreResultExtensions, DB};
use kaspa_core::time::unix_now;
use parking_lot::Mutex;

use stores::banned_address_store::{BannedAddressesStore, BannedAddressesStoreReader, ConnectionBanTimestamp, DbBannedAddressesStore};

pub use stores::NetAddress;

const MAX_ADDRESSES: usize = 4096;
const MAX_CONNECTION_FAILED_COUNT: u64 = 3;

pub struct AddressManager {
    banned_address_store: DbBannedAddressesStore,
    address_store: address_store_with_cache::Store,
}

impl AddressManager {
    pub fn new(db: Arc<DB>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            banned_address_store: DbBannedAddressesStore::new(db.clone(), MAX_ADDRESSES as u64),
            address_store: address_store_with_cache::new(db),
        }))
    }

    pub fn add_address(&mut self, address: NetAddress) {
        // TODO: Don't add non routable addresses

        if self.address_store.has(address) {
            return;
        }

        // We mark `connection_failed_count` as 0 only after first success
        self.address_store.set(address, 1);
    }

    pub fn mark_connection_failure(&mut self, address: NetAddress) {
        if !self.address_store.has(address) {
            return;
        }

        let new_count = self.address_store.get(address).connection_failed_count + 1;
        if new_count > MAX_CONNECTION_FAILED_COUNT {
            self.address_store.remove(address);
        } else {
            self.address_store.set(address, new_count);
        }
    }

    pub fn mark_connection_success(&mut self, address: NetAddress) {
        if !self.address_store.has(address) {
            return;
        }

        self.address_store.set(address, 0);
    }

    pub fn iterate_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
        self.address_store.iterate_addresses()
    }

    pub fn iterate_prioritized_random_addresses(&self, exceptions: HashSet<NetAddress>) -> impl Iterator<Item = NetAddress> {
        self.address_store.iterate_prioritized_random_addresses(exceptions)
    }

    pub fn ban(&mut self, ip: IpAddr) {
        self.banned_address_store.set(ip, ConnectionBanTimestamp(unix_now())).unwrap();
        self.address_store.remove_by_ip(ip);
    }

    pub fn unban(&mut self, ip: IpAddr) {
        self.banned_address_store.remove(ip).unwrap();
    }

    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
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

mod address_store_with_cache {
    // Since we need operations such as iterating all addresses, count, etc, we keep an easy to use copy of the database addresses.
    // We don't expect it to be expensive since we limit the number of saved addresses.
    use std::{
        collections::{HashMap, HashSet},
        net::IpAddr,
        sync::Arc,
    };

    use database::prelude::DB;
    use itertools::Itertools;
    use rand::{
        distributions::{WeightedError, WeightedIndex},
        prelude::Distribution,
    };

    use crate::{
        stores::{
            address_store::{AddressesStore, DbAddressesStore, Entry},
            AddressKey,
        },
        NetAddress, MAX_ADDRESSES, MAX_CONNECTION_FAILED_COUNT,
    };

    pub struct Store {
        db_store: DbAddressesStore,
        addresses: HashMap<AddressKey, Entry>,
    }

    impl Store {
        fn new(db: Arc<DB>) -> Self {
            let db_store = DbAddressesStore::new(db, 0);
            let mut addresses = HashMap::new();
            for (key, entry) in db_store.iterator().map(|res| res.unwrap()) {
                addresses.insert(key, entry);
            }

            Self { db_store, addresses }
        }

        pub fn has(&mut self, address: NetAddress) -> bool {
            self.addresses.contains_key(&address.into())
        }

        pub fn set(&mut self, address: NetAddress, connection_failed_count: u64) {
            let entry = match self.addresses.get(&address.into()) {
                Some(entry) => Entry { connection_failed_count, address: entry.address },
                None => Entry { connection_failed_count, address },
            };
            self.db_store.set(address.into(), entry).unwrap();
            self.addresses.insert(address.into(), entry);
            self.keep_limit();
        }

        fn keep_limit(&mut self) {
            while self.addresses.len() > MAX_ADDRESSES {
                let to_remove =
                    self.addresses.iter().max_by(|a, b| (a.1).connection_failed_count.cmp(&(b.1).connection_failed_count)).unwrap();
                self.remove_by_key(*to_remove.0);
            }
        }

        pub fn get(&self, address: NetAddress) -> Entry {
            *self.addresses.get(&address.into()).unwrap()
        }

        pub fn remove(&mut self, address: NetAddress) {
            self.remove_by_key(address.into())
        }

        fn remove_by_key(&mut self, key: AddressKey) {
            self.addresses.remove(&key);
            self.db_store.remove(key).unwrap()
        }

        pub fn iterate_addresses(&self) -> impl Iterator<Item = NetAddress> + '_ {
            self.addresses.values().map(|entry| entry.address)
        }

        pub fn iterate_prioritized_random_addresses(&self, exceptions: HashSet<NetAddress>) -> impl Iterator<Item = NetAddress> {
            let exceptions: HashSet<AddressKey> = exceptions.into_iter().map(|addr| addr.into()).collect();
            let (weights, addresses) = self
                .addresses
                .iter()
                .filter(|(addr_key, _)| !exceptions.contains(addr_key))
                .map(|(_, e)| (64f64.powf((MAX_CONNECTION_FAILED_COUNT + 1 - e.connection_failed_count) as f64), e.address))
                .unzip();

            RandomWeightedIterator::new(weights, addresses)
        }

        pub fn remove_by_ip(&mut self, ip: IpAddr) {
            for key in self.addresses.keys().filter(|key| key.is_ip(ip)).copied().collect_vec() {
                self.remove_by_key(key);
            }
        }
    }

    pub fn new(db: Arc<DB>) -> Store {
        Store::new(db)
    }

    pub struct RandomWeightedIterator {
        weighted_index: WeightedIndex<f64>,
        addresses: Vec<NetAddress>,
        consumed: bool,
    }

    impl RandomWeightedIterator {
        pub fn new(weights: Vec<f64>, addresses: Vec<NetAddress>) -> Self {
            Self { weighted_index: WeightedIndex::new(weights).unwrap(), addresses, consumed: false }
        }
    }

    impl Iterator for RandomWeightedIterator {
        type Item = NetAddress;

        fn next(&mut self) -> Option<Self::Item> {
            if self.consumed {
                None
            } else {
                let i = self.weighted_index.sample(&mut rand::thread_rng());
                // Zero the selected address entry
                match self.weighted_index.update_weights(&[(i, &0f64)]) {
                    Ok(_) => {}
                    Err(WeightedError::AllWeightsZero) => self.consumed = true,
                    Err(e) => panic!("{e}"),
                }
                Some(self.addresses[i])
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::net::{IpAddr, Ipv6Addr};

        #[test]
        fn test_weighted_iterator() {
            let address = NetAddress::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 1);
            let iter = RandomWeightedIterator::new(vec![0.2, 0.3, 0.0], vec![address, address, address]);
            assert_eq!(iter.count(), 2);
        }
    }
}
