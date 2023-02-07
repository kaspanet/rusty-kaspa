use database::{
    db::DB,
    errors::{StoreError, StoreResult},
    prelude::{CachedDbAccess, DirectDbWriter},
};
use std::net::Ipv6Addr;
use std::{error::Error, fmt::Display, sync::Arc};

const STORE_PREFIX_CONNECTION_FAILED_COUNT: &[u8] = b"not-banned-addresses-connection-failed-count";

pub trait NotBannedAddressesStoreReader {
    fn get(&self, address: Ipv6Addr, port: u16) -> Result<u64, StoreError>;
}

pub trait NotBannedAddressesStore: NotBannedAddressesStoreReader {
    fn set(&mut self, ip: Ipv6Addr, port: u16, connection_failed_count: u64) -> StoreResult<()>;
    fn remove(&mut self, ip: Ipv6Addr, port: u16) -> StoreResult<()>;
}

const IPV6_LEN: usize = 16;
const PORT_LEN: usize = 2;
pub const ADDRESS_KEY_SIZE: usize = IPV6_LEN + PORT_LEN;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct AddressKey([u8; ADDRESS_KEY_SIZE]);

impl AsRef<[u8]> for AddressKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for AddressKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ip_port: (Ipv6Addr, u16) = (*self).into();
        write!(f, "{}:{}", ip_port.0, ip_port.1)
    }
}

impl From<(Ipv6Addr, u16)> for AddressKey {
    fn from(ip_port: (Ipv6Addr, u16)) -> Self {
        let mut bytes = [0; ADDRESS_KEY_SIZE];
        bytes[..IPV6_LEN].copy_from_slice(&ip_port.0.octets());
        bytes[IPV6_LEN..].copy_from_slice(&ip_port.1.to_le_bytes());
        Self(bytes)
    }
}

impl From<AddressKey> for (Ipv6Addr, u16) {
    fn from(k: AddressKey) -> Self {
        let ip_byte_array: [u8; 16] = k.0[..IPV6_LEN].try_into().unwrap();
        let ip: Ipv6Addr = ip_byte_array.into();
        let port_byte_array: [u8; 2] = k.0[IPV6_LEN..].try_into().unwrap();
        let port = u16::from_le_bytes(port_byte_array);
        (ip, port)
    }
}

#[derive(Clone)]
pub struct DbNotBannedAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<AddressKey, u64>,
}

impl DbNotBannedAddressesStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX_CONNECTION_FAILED_COUNT.to_vec()),
        }
    }

    pub fn iterator(&self) -> impl Iterator<Item = Result<((Ipv6Addr, u16), u64), Box<dyn Error>>> + '_ {
        self.access.iterator().map(|iter_result| match iter_result {
            Ok((key_bytes, connection_failed_count)) => match <[u8; ADDRESS_KEY_SIZE]>::try_from(&key_bytes[..]) {
                Ok(address_key_slice) => {
                    let addr_key = AddressKey(address_key_slice);
                    let address: (Ipv6Addr, u16) = addr_key.into();
                    Ok((address, connection_failed_count))
                }
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e),
        })
    }
}

impl NotBannedAddressesStoreReader for DbNotBannedAddressesStore {
    fn get(&self, address: Ipv6Addr, port: u16) -> Result<u64, StoreError> {
        self.access.read((address, port).into())
    }
}

impl NotBannedAddressesStore for DbNotBannedAddressesStore {
    fn set(&mut self, ip: Ipv6Addr, port: u16, connection_failed_count: u64) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), (ip, port).into(), connection_failed_count)
    }

    fn remove(&mut self, ip: Ipv6Addr, port: u16) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), (ip, port).into())
    }
}
