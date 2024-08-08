use kaspa_database::{
    prelude::DB,
    prelude::{CachePolicy, StoreError, StoreResult},
    prelude::{CachedDbAccess, DirectDbWriter},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::net::Ipv6Addr;
use std::{error::Error, fmt::Display, sync::Arc};

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

const IPV6_LEN: usize = 16;
const PORT_LEN: usize = 2;
pub const ADDRESS_KEY_SIZE: usize = IPV6_LEN + PORT_LEN;

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
        write!(f, "{}:{}", ip_port.0, ip_port.1)
    }
}

impl From<AddressKey> for DbAddressKey {
    fn from(key: AddressKey) -> Self {
        let mut bytes = [0; ADDRESS_KEY_SIZE];
        bytes[..IPV6_LEN].copy_from_slice(&key.0.octets());
        bytes[IPV6_LEN..].copy_from_slice(&key.1.to_le_bytes());
        Self(bytes)
    }
}

impl From<DbAddressKey> for AddressKey {
    fn from(k: DbAddressKey) -> Self {
        let ip_byte_array: [u8; 16] = k.0[..IPV6_LEN].try_into().unwrap();
        let ip: Ipv6Addr = ip_byte_array.into();
        let port_byte_array: [u8; 2] = k.0[IPV6_LEN..].try_into().unwrap();
        let port = u16::from_le_bytes(port_byte_array);
        AddressKey::new(ip, port)
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
            Ok((key_bytes, connection_failed_count)) => match <[u8; ADDRESS_KEY_SIZE]>::try_from(&key_bytes[..]) {
                Ok(address_key_slice) => {
                    let addr_key = DbAddressKey(address_key_slice);
                    let address: AddressKey = addr_key.into();
                    Ok((address, connection_failed_count))
                }
                Err(e) => Err(e.into()),
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
