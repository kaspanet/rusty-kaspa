mod stores;

extern crate self as address_manager;

use std::{collections::HashSet, sync::Arc};

use itertools::Itertools;
use kaspa_core::{debug, time::unix_now};
use kaspa_database::prelude::{StoreResultExtensions, DB};
use kaspa_utils::networking::IpAddress;
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
        if address.ip.is_loopback() || address.ip.is_unspecified() {
            debug!("[Address manager] skipping local address {}", address.ip);
            return;
        }

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

    pub fn iterate_prioritized_random_addresses(&self, exceptions: HashSet<NetAddress>) -> impl ExactSizeIterator<Item = NetAddress> {
        self.address_store.iterate_prioritized_random_addresses(exceptions)
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
        match self.banned_address_store.get(ip.into()).unwrap_option() {
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

    pub fn get_all_addresses(&self) -> Vec<NetAddress> {
        self.address_store.iterate_addresses().collect_vec()
    }

    pub fn get_all_banned_addresses(&self) -> Vec<IpAddress> {
        self.banned_address_store.iterator().map(|x| IpAddress::from(x.unwrap().0)).collect_vec()
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

    use itertools::Itertools;
    use kaspa_database::prelude::DB;
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

        pub fn iterate_prioritized_random_addresses(
            &self,
            exceptions: HashSet<NetAddress>,
        ) -> impl ExactSizeIterator<Item = NetAddress> {
            let exceptions: HashSet<AddressKey> = exceptions.into_iter().map(|addr| addr.into()).collect();
            let mut prefix_counter: HashMap<Vec<u8>, usize> = HashMap::new();
            let (mut weights, filtered_addresses): (Vec<f64>, Vec<NetAddress>) = self
                .addresses
                .iter()
                .filter(|(addr_key, _)| !exceptions.contains(addr_key))
                .map(|(_, e)| {
                    let count = prefix_counter.entry(e.address.clone().prefix_bytes()).or_insert(0);
                    *count += 1;
                    (64f64.powf((MAX_CONNECTION_FAILED_COUNT + 1 - e.connection_failed_count) as f64), e.address)
                })
                .unzip();

            // divide weights by number routing prefix, and subnetwork bytes to normalize distribution over network
            for (i, address) in filtered_addresses.iter().enumerate() {
                weights[i] = weights[i] / (*prefix_counter.get_mut(&address.prefix_bytes()).unwrap() as f64);
            }

            RandomWeightedIterator::new(weights, filtered_addresses)
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
        weighted_index: Option<WeightedIndex<f64>>,
        remaining: usize,
        addresses: Vec<NetAddress>,
    }

    impl RandomWeightedIterator {
        pub fn new(weights: Vec<f64>, addresses: Vec<NetAddress>) -> Self {
            assert_eq!(weights.len(), addresses.len());
            let remaining = weights.iter().filter(|&&w| w > 0.0).count();
            let weighted_index = match WeightedIndex::new(weights) {
                Ok(index) => Some(index),
                Err(WeightedError::NoItem) => None,
                Err(e) => panic!("{e}"),
            };
            Self { weighted_index, remaining, addresses }
        }
    }

    impl Iterator for RandomWeightedIterator {
        type Item = NetAddress;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(weighted_index) = self.weighted_index.as_mut() {
                let i = weighted_index.sample(&mut rand::thread_rng());
                // Zero the selected address entry
                match weighted_index.update_weights(&[(i, &0f64)]) {
                    Ok(_) => {}
                    Err(WeightedError::AllWeightsZero) => self.weighted_index = None,
                    Err(e) => panic!("{e}"),
                }
                self.remaining -= 1;
                Some(self.addresses[i])
            } else {
                None
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.remaining, Some(self.remaining))
        }
    }

    impl ExactSizeIterator for RandomWeightedIterator {}

    #[cfg(test)]
    mod tests {
        use super::*;
        use address_manager::AddressManager;
        use ipgen::{ip, IpNetwork};
        use kaspa_database::utils::create_temp_db;
        use kaspa_utils::networking::IpAddress;
        use statest::ttest::{ttest1, Side, UPLO};
        use std::net::{IpAddr, Ipv6Addr};

        #[test]
        fn test_weighted_iterator() {
            let address = NetAddress::new(IpAddr::V6(Ipv6Addr::LOCALHOST).into(), 1);
            let iter = RandomWeightedIterator::new(vec![0.2, 0.3, 0.0], vec![address, address, address]);
            assert_eq!(iter.len(), 2);
            assert_eq!(iter.count(), 2);

            let iter = RandomWeightedIterator::new(vec![], vec![]);
            assert_eq!(iter.len(), 0);
            assert_eq!(iter.count(), 0);
        }

        #[test]
        fn test_network_distribution_weighting() {
            let addresses = vec![
                // 12 addresses with 0x01, 0x01 as prefix bytes
                ip("a", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("b", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("c", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("d", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("e", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("f", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("g", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("h", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("i", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("j", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("k", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("l", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("m", "1.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                // 6 addresses with 0x01, 0x02 as prefix bytes
                ip("n", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("o", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("p", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("q", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("r", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("s", "1.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                // 3 addresses with 0x02, 0x01 as prefix bytes
                ip("w", "2.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("x", "2.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                ip("y", "2.1.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
                // 1 address with 0x02, 0x02 as prefix bytes
                ip("z", "2.2.0.0/16".parse::<IpNetwork>().unwrap()).unwrap(),
            ];
            let db = create_temp_db();
            let am = AddressManager::new(db.1);
            addresses.iter().for_each(|address| am.lock().add_address(NetAddress { ip: IpAddress::new(*address), port: 16111}));

            let execptions = HashSet::new();
            let selected_addresses = am.lock().iterate_prioritized_random_addresses(execptions).collect_vec();
            assert_eq!(selected_addresses.len(), addresses.len());
            
            // Set-up for T-test
            let mut mean: f64 = 0.0;

            let dist_selected = selected_addresses.into_iter().map(|address| {
                let prefix_bytes = address.prefix_bytes();
                let ord = match prefix_bytes[0] {
                    1 => match prefix_bytes[1] {
                        1 => 0.0,
                        2 => 1.0,
                        _ => panic!("unexpected byte"),
                    },
                    2 => match prefix_bytes[1] {
                        1 => 2.0,
                        2 => 3.0,
                        _ => panic!("unexpected byte"),
                    },
                    _ => panic!("unexpected byte"),
                };
                mean += ord;
                ord
            }).collect_vec();

            // assert overall mean deviates significantly from first half of weighted addresses.
            assert!(ttest1(&dist_selected.as_slice()[0..addresses.len()], mean, 0.05, Side::One(UPLO::Upper)));
      
        }
    }
}
