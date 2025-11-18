use kaspa_database::{
    prelude::DB,
    prelude::{CachePolicy, StoreError, StoreResult},
    prelude::{CachedDbAccess, DirectDbWriter},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::{
    mem_size::MemSizeEstimator,
    networking::{AddressKind, IpAddress, OnionAddress},
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv6Addr};
use std::{convert::TryInto, error::Error, fmt::Display, sync::Arc};

use super::AddressKey;
use crate::NetAddress;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Entry {
    pub connection_failed_count: u64,
    pub address: NetAddress,
}

impl MemSizeEstimator for Entry {}

pub trait AddressesStoreReader {
    #[allow(dead_code)]
    fn get(&self, key: AddressKey) -> Result<Entry, StoreError>;
}

pub trait AddressesStore: AddressesStoreReader {
    fn set(&mut self, key: AddressKey, entry: Entry) -> StoreResult<()>;
    #[allow(dead_code)]
    fn set_failed_count(&mut self, key: AddressKey, connection_failed_count: u64) -> StoreResult<()>;
    fn remove(&mut self, key: AddressKey) -> StoreResult<()>;
}

const ADDRESS_KIND_TAG_LEN: usize = 1;
const IPV6_LEN: usize = 16;
const ONION_LEN: usize = 35;
const ADDRESS_DATA_LEN: usize = ONION_LEN;
const PORT_LEN: usize = 2;
const LEGACY_ADDRESS_KEY_SIZE: usize = IPV6_LEN + PORT_LEN;
pub const ADDRESS_KEY_SIZE: usize = ADDRESS_KIND_TAG_LEN + ADDRESS_DATA_LEN + PORT_LEN;

// TODO: This pattern is used a lot. Think of some macro or any other way to generalize it.
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct DbAddressKey([u8; ADDRESS_KEY_SIZE]);

impl AsRef<[u8]> for DbAddressKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for DbAddressKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ip_port: AddressKey = (*self).into();
        match ip_port.kind() {
            AddressKind::Ip(ip) => write!(f, "{}:{}", IpAddr::from(ip), ip_port.port()),
            AddressKind::Onion(onion) => write!(f, "{}:{}", onion, ip_port.port()),
        }
    }
}

impl From<AddressKey> for DbAddressKey {
    fn from(key: AddressKey) -> Self {
        let mut bytes = [0; ADDRESS_KEY_SIZE];
        match key.kind() {
            AddressKind::Ip(ip) => {
                bytes[0] = 0;
                let ip_addr = match IpAddr::from(ip) {
                    IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
                    IpAddr::V6(ipv6) => ipv6,
                };
                bytes[1..1 + IPV6_LEN].copy_from_slice(&ip_addr.octets());
            }
            AddressKind::Onion(onion) => {
                bytes[0] = 1;
                let raw = onion.raw();
                bytes[1..1 + raw.len()].copy_from_slice(raw);
            }
        }
        let port_bytes = key.port().to_le_bytes();
        bytes[ADDRESS_KEY_SIZE - PORT_LEN..].copy_from_slice(&port_bytes);
        Self(bytes)
    }
}

impl From<DbAddressKey> for AddressKey {
    fn from(k: DbAddressKey) -> Self {
        let tag = k.0[0];
        let port_offset = ADDRESS_KEY_SIZE - PORT_LEN;
        let port_bytes: [u8; PORT_LEN] = k.0[port_offset..].try_into().unwrap();
        let port = u16::from_le_bytes(port_bytes);
        match tag {
            0 => {
                let mut ipv6_bytes = [0u8; IPV6_LEN];
                ipv6_bytes.copy_from_slice(&k.0[1..1 + IPV6_LEN]);
                let ipv6 = Ipv6Addr::from(ipv6_bytes);
                let ip = ipv6.to_ipv4().map_or(IpAddr::V6(ipv6), IpAddr::V4);
                AddressKey::new(AddressKind::Ip(IpAddress::from(ip)), port)
            }
            1 => {
                let mut raw = [0u8; ONION_LEN];
                raw.copy_from_slice(&k.0[1..1 + ONION_LEN]);
                let onion = OnionAddress::from_raw(raw);
                AddressKey::new(AddressKind::Onion(onion), port)
            }
            other => panic!("invalid address key variant {}", other),
        }
    }
}

#[derive(Clone)]
pub struct DbAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<DbAddressKey, Entry>,
}

impl DbAddressesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::Addresses.into()) }
    }

    pub fn iterator(&self) -> impl Iterator<Item = Result<(AddressKey, Entry), Box<dyn Error>>> + '_ {
        self.access.iterator().map(|iter_result| match iter_result {
            Ok((key_bytes, connection_failed_count)) => match key_bytes.len() {
                ADDRESS_KEY_SIZE => {
                    let address_key_slice: [u8; ADDRESS_KEY_SIZE] = key_bytes[..].try_into().expect("slice size checked");
                    let addr_key = DbAddressKey(address_key_slice);
                    let address: AddressKey = addr_key.into();
                    Ok((address, connection_failed_count))
                }
                LEGACY_ADDRESS_KEY_SIZE => {
                    let port_offset = LEGACY_ADDRESS_KEY_SIZE - PORT_LEN;
                    let ipv6_bytes: [u8; IPV6_LEN] = key_bytes[..IPV6_LEN].try_into().expect("slice size checked");
                    let port_bytes: [u8; PORT_LEN] = key_bytes[port_offset..].try_into().expect("slice size checked");
                    let ipv6 = Ipv6Addr::from(ipv6_bytes);
                    let ip = ipv6.to_ipv4().map_or(IpAddr::V6(ipv6), IpAddr::V4);
                    let address = AddressKey::new(AddressKind::Ip(IpAddress::from(ip)), u16::from_le_bytes(port_bytes));
                    Ok((address, connection_failed_count))
                }
                len => Err(format!("invalid address key length {}", len).into()),
            },
            Err(e) => Err(e),
        })
    }
}

impl AddressesStoreReader for DbAddressesStore {
    fn get(&self, key: AddressKey) -> Result<Entry, StoreError> {
        self.access.read(key.into())
    }
}

impl AddressesStore for DbAddressesStore {
    fn set(&mut self, key: AddressKey, entry: Entry) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), key.into(), entry)
    }

    fn remove(&mut self, key: AddressKey) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), key.into())
    }

    fn set_failed_count(&mut self, key: AddressKey, connection_failed_count: u64) -> StoreResult<()> {
        let entry = self.get(key)?;
        self.set(key, Entry { connection_failed_count, address: entry.address })
    }
}

impl DbAddressesStore {
    pub fn clear(&self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}
