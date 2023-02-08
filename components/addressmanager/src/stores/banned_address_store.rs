use database::{
    prelude::{CachedDbAccess, DirectDbWriter, DB},
    prelude::{StoreError, StoreResult},
};
use std::net::Ipv6Addr;
use std::{fmt::Display, sync::Arc};

const STORE_PREFIX: &[u8] = b"banned-addresses";

pub trait BannedAddressesStoreReader {
    fn get(&self, address: Ipv6Addr) -> Result<u64, StoreError>;
}

pub trait BannedAddressesStore: BannedAddressesStoreReader {
    fn set(&mut self, ip: Ipv6Addr, timestamp: u64) -> StoreResult<()>;
    fn remove(&mut self, ip: Ipv6Addr) -> StoreResult<()>;
}

const IPV6_LEN: usize = 16;
const ADDRESS_KEY_SIZE: usize = IPV6_LEN;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct AddressKey([u8; ADDRESS_KEY_SIZE]);

impl AsRef<[u8]> for AddressKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for AddressKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ip: Ipv6Addr = (*self).into();
        write!(f, "{ip}")
    }
}

impl From<Ipv6Addr> for AddressKey {
    fn from(ip: Ipv6Addr) -> Self {
        Self(ip.octets())
    }
}

impl From<AddressKey> for Ipv6Addr {
    fn from(k: AddressKey) -> Self {
        k.0.into()
    }
}

#[derive(Clone)]
pub struct DbBannedAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<AddressKey, u64>,
}

impl DbBannedAddressesStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX.to_vec()) }
    }
}

impl BannedAddressesStoreReader for DbBannedAddressesStore {
    fn get(&self, ip: Ipv6Addr) -> Result<u64, StoreError> {
        self.access.read(ip.into())
    }
}

impl BannedAddressesStore for DbBannedAddressesStore {
    fn set(&mut self, ip: Ipv6Addr, timestamp: u64) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), ip.into(), timestamp)
    }

    fn remove(&mut self, ip: Ipv6Addr) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), ip.into())
    }
}
