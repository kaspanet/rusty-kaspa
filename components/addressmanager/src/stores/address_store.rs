use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreError, StoreResult},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt::Display, sync::Arc};
use std::{mem, net::Ipv6Addr};

use super::AddressKey;
use crate::NetAddress;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Entry {
    pub connection_failed_count: u64,
    pub address: NetAddress,
}

impl MemSizeEstimator for Entry {}

/// Address entry for persisted leveraged perigee addresses
/// the rank indicates the quality of the address (lower is better)
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PerigeeEntry {
    pub rank: u16,
    pub address: NetAddress,
}

impl MemSizeEstimator for PerigeeEntry {}

pub trait AddressesStoreReader {
    #[allow(dead_code)]
    fn get(&self, key: AddressKey) -> Result<Entry, StoreError>;
    fn get_perigee_addresses(&self) -> Result<Vec<NetAddress>, StoreError>;
}

pub trait AddressesStore: AddressesStoreReader {
    fn set(&mut self, key: AddressKey, entry: Entry) -> StoreResult<()>;
    fn set_new_perigee_addresses(&mut self, entries: Vec<NetAddress>) -> StoreResult<()>;
    #[allow(dead_code)]
    fn set_failed_count(&mut self, key: AddressKey, connection_failed_count: u64) -> StoreResult<()>;
    fn remove(&mut self, key: AddressKey) -> StoreResult<()>;
    fn reset_perigee_data(&mut self) -> StoreResult<()>;
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

impl From<NetAddress> for DbAddressKey {
    fn from(address: NetAddress) -> Self {
        AddressKey::from(address).into()
    }
}

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct DbPerigeeRankedAddressKey([u8; mem::size_of::<u16>() + ADDRESS_KEY_SIZE]);

impl From<PerigeeEntry> for DbPerigeeRankedAddressKey {
    fn from(perigee_entry: PerigeeEntry) -> Self {
        let mut bytes = [0; mem::size_of::<DbPerigeeRankedAddressKey>()];
        bytes[..mem::size_of::<u16>()].copy_from_slice(&perigee_entry.rank.to_be_bytes()); // big-endian for lexicographic ordering in rocks db, it is important
        bytes[mem::size_of::<u16>()..].copy_from_slice(DbAddressKey::from(perigee_entry.address).as_ref());
        Self(bytes)
    }
}

impl From<DbPerigeeRankedAddressKey> for PerigeeEntry {
    fn from(db_key: DbPerigeeRankedAddressKey) -> Self {
        let rank_byte_array = db_key.0[..mem::size_of::<u16>()].try_into().unwrap();
        let rank = u16::from_le_bytes(rank_byte_array);
        let address_key_bytes: [u8; ADDRESS_KEY_SIZE] = db_key.0[mem::size_of::<u16>()..].try_into().unwrap();
        let address_key = DbAddressKey(address_key_bytes);
        let address = AddressKey::from(address_key).into();
        PerigeeEntry { rank, address }
    }
}

impl AsRef<[u8]> for DbPerigeeRankedAddressKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone)]
pub struct DbAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<DbAddressKey, Entry>,
    perigee_access: CachedDbAccess<DbPerigeeRankedAddressKey, PerigeeEntry>,
}

impl DbAddressesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::Addresses.into()),
            perigee_access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PerigeeAddresses.into()),
        }
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

    /// Get persisted leveraged perigee addresses, ordered by their ascending perigee rank (low is better).
    fn get_perigee_addresses(&self) -> StoreResult<Vec<NetAddress>> {
        self.perigee_access
            .iterator()
            .map(|res| {
                res.map(|(_, perigee_entry)| perigee_entry.address).map_err(|err| StoreError::DataInconsistency(err.to_string()))
            })
            .collect::<StoreResult<Vec<NetAddress>>>()
    }
}

impl AddressesStore for DbAddressesStore {
    fn set(&mut self, key: AddressKey, entry: Entry) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), key.into(), entry)
    }

    /// Replaces all existing persisted perigee addresses with the given set of addresses.
    /// note: the order of the given addresses determines their perigee rank (low is better).
    /// this is important for order of retrieval of leveraged perigee addresses between restarts.
    /// in cases where the number of required addresses decreases, only the top addresses are chosen.
    fn set_new_perigee_addresses(&mut self, entries: Vec<NetAddress>) -> StoreResult<()> {
        // First, clear existing perigee addresses
        self.perigee_access.delete_all(DirectDbWriter::new(&self.db))?;

        let mut key_iter = entries.iter().enumerate().map(|(rank, address)| {
            let perigee_entry = PerigeeEntry { rank: rank as u16, address: *address };
            let db_key = DbPerigeeRankedAddressKey::from(perigee_entry);
            (db_key, perigee_entry)
        });

        self.perigee_access.write_many(DirectDbWriter::new(&self.db), &mut key_iter)
    }

    fn remove(&mut self, key: AddressKey) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), key.into())
    }

    fn set_failed_count(&mut self, key: AddressKey, connection_failed_count: u64) -> StoreResult<()> {
        let entry = self.get(key)?;
        self.set(key, Entry { connection_failed_count, address: entry.address })
    }

    fn reset_perigee_data(&mut self) -> StoreResult<()> {
        self.perigee_access.delete_all(DirectDbWriter::new(&self.db))
    }
}
