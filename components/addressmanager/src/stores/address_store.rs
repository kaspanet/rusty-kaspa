use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, CachedDbItem, DB, DbKey, DirectDbWriter, StoreError, StoreResult, StoreResultExt},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::{
    connection::{AddressKey, LibP2pAddress, Libp2pId, Libp2pMetadata, PeerAddress},
    mem_size::MemSizeEstimator,
    networking::NetAddress,
};
use rocksdb::IterateBounds;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv6Addr},
    sync::Arc,
};

/// Frozen layout of master address values, read only during migration.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct LegacyEntry {
    pub connection_failed_count: u64,
    pub address: NetAddress,
}

impl MemSizeEstimator for LegacyEntry {}

/// Advertised fields not already in the key, plus local verification and failure state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub relay_port: Option<u16>,
    pub libp2p_port: Option<u16>,
    pub relay_address: Option<LibP2pAddress>,
    pub nonce: u64,
    pub signature: Option<Vec<u8>>,
    /// Whether the current advertisement has passed signature verification locally.
    pub signature_verified: bool,
    pub consecutive_relay_failures: u64,
    pub connection_failed_count: u64,
}

impl MemSizeEstimator for Entry {}

impl Entry {
    pub fn address(&self, key: AddressKey) -> PeerAddress {
        PeerAddress {
            ip: key.0.ip,
            tcp_port: key.0.port,
            libp2p: key.1.map(|endpoint_id| Libp2pMetadata {
                endpoint_id,
                port: self.libp2p_port.unwrap_or_default(),
                relay_port: self.relay_port.unwrap_or_default(),
                nonce: self.nonce,
                signature: self.signature.clone().unwrap_or_default(),
            }),
            relay_address: self.relay_address,
        }
    }

    pub fn update(&mut self, address: &PeerAddress, signature_verified: bool) {
        self.relay_port = address.relay_port();
        self.libp2p_port = address.libp2p_port();
        self.relay_address = address.relay_address;
        self.nonce = address.nonce().unwrap_or_default();
        self.signature = address.signature().map(|signature| signature.to_vec());
        self.signature_verified = signature_verified;
    }
}

fn ip_prefix(ip: IpAddr) -> [u8; 16] {
    match ip {
        IpAddr::V4(ip) => ip.to_ipv6_mapped().octets(),
        IpAddr::V6(ip) => ip.octets(),
    }
}

const KEY_WITH_ID_LEN: usize = 19 + size_of::<Libp2pId>();

/// TCP socket is the prefix, followed by the optional signing identity.
fn key(address: &AddressKey) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(KEY_WITH_ID_LEN);
    bytes.extend(ip_prefix(address.0.ip.0));
    bytes.extend(address.0.port.to_le_bytes());
    bytes.push(u8::from(address.1.is_some()));
    if let Some(id) = address.1 {
        bytes.extend(id);
    }
    bytes
}

fn decode_key(bytes: &[u8]) -> StoreResult<AddressKey> {
    if bytes.len() != 19 && bytes.len() != KEY_WITH_ID_LEN {
        return Err(StoreError::DataInconsistency("invalid signed address key length".into()));
    }
    let ip = IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[..16]).unwrap())).to_canonical();
    let port = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
    let id = match (bytes[18], bytes.len()) {
        (0, 19) => None,
        (1, KEY_WITH_ID_LEN) => Some(bytes[19..].try_into().unwrap()),
        _ => return Err(StoreError::DataInconsistency("invalid signed address identity tag".into())),
    };
    Ok((NetAddress::new(ip.into(), port), id))
}

pub struct DbAddressesStore {
    db: Arc<DB>,
    access: CachedDbAccess<Vec<u8>, Entry>,
    local_advertisement: CachedDbItem<PeerAddress>,
}

impl DbAddressesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::Addresses.into()),
            local_advertisement: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::LocalAddressAdvertisement.into()),
            db,
        }
    }

    pub(crate) fn local_advertisement(&self) -> StoreResult<Option<PeerAddress>> {
        self.local_advertisement.read().optional()
    }

    pub(crate) fn set_local_advertisement(&mut self, address: &PeerAddress) -> StoreResult<()> {
        self.local_advertisement.write(DirectDbWriter::new(&self.db), address)
    }

    /// Copies master TCP records, retaining connection failures. Extended fields are absent until learned from an owner.
    /// The source prefixes are cleared atomically with the destination writes.
    pub fn migrate(&self) -> StoreResult<()> {
        let legacy_access = CachedDbAccess::<Vec<u8>, LegacyEntry>::new(
            self.db.clone(),
            CachePolicy::Empty,
            DatabaseStorePrefixes::AddressesLegacy.into(),
        );
        let mut batch = rocksdb::WriteBatch::default();
        for row in legacy_access.iterator() {
            let (bytes, legacy) =
                row.map_err(|error| StoreError::DataInconsistency(format!("failed reading legacy address: {error}")))?;
            let entry = Entry { connection_failed_count: legacy.connection_failed_count, ..Default::default() };
            batch
                .put(DbKey::new(DatabaseStorePrefixes::Addresses.as_ref(), key(&(legacy.address, None))), bincode::serialize(&entry)?);
            batch.delete(DbKey::new(DatabaseStorePrefixes::AddressesLegacy.as_ref(), bytes));
        }
        self.db.write(batch)?;
        Ok(())
    }

    pub fn iterator(&self) -> impl Iterator<Item = StoreResult<(AddressKey, Entry)>> + '_ {
        self.access.iterator().map(|row| {
            let (bytes, entry) = row.map_err(|error| StoreError::DataInconsistency(format!("failed reading address: {error}")))?;
            Ok((decode_key(&bytes)?, entry))
        })
    }

    pub fn iterator_by_ip(&self, ip: IpAddr) -> impl Iterator<Item = StoreResult<(AddressKey, Entry)>> + '_ {
        let (start, end) = rocksdb::PrefixRange(ip_prefix(ip)).into_bounds();
        self.access.seek_iterator(None, start, end, usize::MAX, false).map(|row| {
            let (bytes, entry) = row.map_err(|error| StoreError::DataInconsistency(format!("failed reading address: {error}")))?;
            Ok((decode_key(&bytes)?, entry))
        })
    }

    pub fn set(&self, address: &AddressKey, entry: &Entry) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), key(address), entry.clone())
    }

    pub fn remove(&self, address: &AddressKey) -> StoreResult<()> {
        self.access.delete(DirectDbWriter::new(&self.db), key(address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};

    #[test]
    fn migration_preserves_master_addresses_and_counters_and_is_repeatable() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let legacy =
            CachedDbAccess::<Vec<u8>, LegacyEntry>::new(db.clone(), CachePolicy::Empty, DatabaseStorePrefixes::AddressesLegacy.into());
        let tcp: NetAddress = "8.8.8.8:16111".parse().unwrap();
        legacy.write(DirectDbWriter::new(&db), vec![1], LegacyEntry { address: tcp, connection_failed_count: 2 }).unwrap();
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        store.migrate().unwrap();
        store.migrate().unwrap();
        let records = store.iterator().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(records, vec![((tcp, None), Entry { connection_failed_count: 2, ..Default::default() })]);
        assert_eq!(legacy.iterator().count(), 0);
        assert_eq!(records[0].1.address(records[0].0), PeerAddress::from(tcp));
    }

    #[test]
    fn corrupt_legacy_values_leave_all_sources_untouched() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let source = DbKey::new(DatabaseStorePrefixes::AddressesLegacy.as_ref(), [1]);
        db.put(&source, [1]).unwrap();
        let store = DbAddressesStore::new(db.clone(), CachePolicy::Empty);
        assert!(store.migrate().is_err());
        assert!(db.get(source).unwrap().is_some());
        assert_eq!(store.iterator().count(), 0);
    }

    #[test]
    fn key_has_tcp_prefix_and_identity_and_round_trips_ipv4_aliases() {
        let a: NetAddress = "8.8.8.8:16111".parse().unwrap();
        let alias: NetAddress = "[::ffff:8.8.8.8]:16111".parse().unwrap();
        for id in [None, Some([1; 32]), Some([2; 32])] {
            assert_eq!(key(&(a, id)), key(&(alias, id)));
            assert_eq!(decode_key(&key(&(a, id))).unwrap(), (a, id));
        }
        assert_ne!(key(&(a, None)), key(&(a, Some([1; 32]))));
        assert_ne!(key(&(a, Some([1; 32]))), key(&(a, Some([2; 32]))));
        assert!(decode_key(&[]).is_err());
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbAddressesStore::new(db, CachePolicy::Empty);
        for address in [(a, None), (a, Some([1; 32])), ("1.1.1.1:16111".parse().unwrap(), None)] {
            store.set(&address, &Entry::default()).unwrap();
        }
        assert_eq!(store.iterator_by_ip(alias.ip.into()).count(), 2);
    }
}
