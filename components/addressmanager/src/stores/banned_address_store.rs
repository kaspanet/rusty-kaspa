use kaspa_database::{
    prelude::{CachePolicy, StoreError, StoreResult},
    prelude::{CachedDbAccess, DirectDbWriter, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv6Addr};
use std::{error::Error, fmt::Display, sync::Arc};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ConnectionBanTimestamp(pub u64);

impl MemSizeEstimator for ConnectionBanTimestamp {}

pub trait BannedAddressesStoreReader {
    fn get(&self, address: IpAddr) -> Result<ConnectionBanTimestamp, StoreError>;
}

pub trait BannedAddressesStore: BannedAddressesStoreReader {
    fn set(&mut self, ip: IpAddr, timestamp: ConnectionBanTimestamp) -> StoreResult<()>;
    fn remove(&mut self, ip: IpAddr) -> StoreResult<()>;
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

impl From<IpAddr> for AddressKey {
    fn from(ip: IpAddr) -> Self {
        Self(match ip {
            IpAddr::V4(ip) => ip.to_ipv6_mapped().octets(),
            IpAddr::V6(ip) => ip.octets(),
        })
    }
}

impl From<AddressKey> for Ipv6Addr {
    fn from(k: AddressKey) -> Self {
        k.0.into()
    }
}

impl From<AddressKey> for IpAddr {
    fn from(k: AddressKey) -> Self {
        let ipv6: Ipv6Addr = k.0.into();
        match ipv6.to_ipv4_mapped() {
            Some(ipv4) => IpAddr::V4(ipv4),
            None => IpAddr::V6(ipv6),
        }
    }
}

#[derive(Clone)]
pub struct DbBannedAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<AddressKey, ConnectionBanTimestamp>,
}

impl DbBannedAddressesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::BannedAddresses.into()) }
    }

    pub fn iterator(&self) -> impl Iterator<Item = Result<(IpAddr, ConnectionBanTimestamp), Box<dyn Error>>> + '_ {
        self.access.iterator().map(|iter_result| match iter_result {
            Ok((key_bytes, connection_ban_timestamp)) => match <[u8; ADDRESS_KEY_SIZE]>::try_from(&key_bytes[..]) {
                Ok(address_key_slice) => {
                    let addr_key = AddressKey(address_key_slice);
                    let address: IpAddr = addr_key.into();
                    Ok((address, connection_ban_timestamp))
                }
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e),
        })
    }
}

impl BannedAddressesStoreReader for DbBannedAddressesStore {
    fn get(&self, ip: IpAddr) -> Result<ConnectionBanTimestamp, StoreError> {
        self.access.read(ip.into())
    }
}

impl BannedAddressesStore for DbBannedAddressesStore {
    fn set(&mut self, ip: IpAddr, timestamp: ConnectionBanTimestamp) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), ip.into(), timestamp)
    }

    fn remove(&mut self, ip: IpAddr) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), ip.into())
    }
}
