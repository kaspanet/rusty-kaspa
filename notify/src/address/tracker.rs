use crate::address::error::{Error, Result};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::{debug, info, trace};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    collections::{hash_map, hash_set, HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

pub trait Indexer {
    /// The tracker used internally to register/unregister/lookup addresses
    fn tracker(&self) -> &Tracker;

    fn contains(&self, index: Index) -> bool;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

trait IndexerStorage {
    /// Inserts an [`Index`].
    ///
    /// Returns true if the index was not present and was successfully inserted, false otherwise.
    fn insert(&mut self, index: Index) -> bool;

    /// Removes an [`Index`].
    ///
    /// Returns true if the index was present and successfully removed, false otherwise.
    fn remove(&mut self, index: Index) -> bool;
}

pub type Index = u32;
pub type RefCount = u16;

/// Tracks reference count of indexes
pub type Counters = CounterMap;

/// Tracks indexes
pub type Indexes = IndexSet;

/// Tracks reference count of indexes
#[derive(Debug)]
pub struct CounterMap {
    /// Tracker storing the addresses
    tracker: Tracker,

    /// Map of address index to reference count
    indexes: HashMap<Index, RefCount>,
}

impl CounterMap {
    pub fn new(tracker: Tracker) -> Self {
        Self { tracker, indexes: HashMap::new() }
    }

    pub fn with_capacity(tracker: Tracker, capacity: usize) -> Self {
        Self { tracker, indexes: HashMap::with_capacity(capacity) }
    }

    /// Registers a vector of addresses, inserting them if not existing yet and increasing their reference count.
    ///
    /// On success, returns the addresses that were actually inserted.
    ///
    /// Fails with [`Error::MaxCapacityReached`] if the maximum capacity of the underlying [`Tracker`] gets reached,
    /// leaving both the tracker and the object unchanged.
    pub fn register(&mut self, addresses: Vec<Address>) -> Result<Vec<Address>> {
        self.tracker.clone().register(self, addresses)
    }

    /// Unregisters a vector of addresses, decreasing their reference count when existing in the map and removing them
    /// when their reference count reaches zero.
    ///
    /// Returns the addresses that where successfully removed.
    pub fn unregister(&mut self, addresses: Vec<Address>) -> Vec<Address> {
        self.tracker.clone().unregister(self, addresses)
    }

    /// Unregisters all indexes, draining the map in the process.
    pub fn clear(&mut self) {
        self.tracker.clone().clear_counters(self)
    }

    pub fn iter(&self) -> hash_map::Iter<'_, Index, RefCount> {
        self.indexes.iter()
    }

    pub fn len(&self) -> usize {
        self.indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.indexes.capacity()
    }

    #[cfg(test)]
    pub fn test_direct_insert(&mut self, index: Index) -> bool {
        self.insert(index)
    }
}

impl Indexer for CounterMap {
    fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    fn contains(&self, index: Index) -> bool {
        self.indexes.contains_key(&index)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl IndexerStorage for CounterMap {
    fn insert(&mut self, index: Index) -> bool {
        let mut result = true;
        self.indexes
            .entry(index)
            .and_modify(|x| {
                *x += 1;
                result = *x == 1;
            })
            .or_insert(1);
        result
    }

    fn remove(&mut self, index: Index) -> bool {
        let mut result = false;
        self.indexes.entry(index).and_modify(|x| {
            if *x > 0 {
                *x -= 1;
                result = *x == 0
            }
        });
        result
    }
}

impl Clone for CounterMap {
    fn clone(&self) -> Self {
        // Clone the indexes...
        let indexes = self.indexes.clone();
        // ...and make sure that the tracker increases the reference count of each cloned index
        self.tracker.reference_indexes(indexes.keys());

        Self { tracker: self.tracker.clone(), indexes }
    }
}

impl Drop for CounterMap {
    fn drop(&mut self) {
        // Decreases the tracker indexes reference count
        self.clear()
    }
}

impl PartialEq for CounterMap {
    fn eq(&self, other: &Self) -> bool {
        self.indexes == other.indexes
    }
}
impl Eq for CounterMap {}

/// Set of `Index`
#[derive(Debug)]
pub struct IndexSet {
    /// Tracker storing the addresses
    tracker: Tracker,

    /// Set of address index
    indexes: HashSet<Index>,
}

impl IndexSet {
    pub fn new(tracker: Tracker, indexes: Vec<Index>) -> Self {
        Self { tracker, indexes: indexes.into_iter().collect() }
    }

    pub fn with_capacity(tracker: Tracker, capacity: usize) -> Self {
        Self { tracker, indexes: HashSet::with_capacity(capacity) }
    }

    /// Registers a vector of addresses, inserting them if not existing yet.
    ///
    /// On success, returns the addresses that were actually inserted.
    ///
    /// Fails with [`Error::MaxCapacityReached`] if the maximum capacity of the underlying [`Tracker`] gets reached,
    /// leaving both the tracker and the object unchanged.
    pub fn register(&mut self, addresses: Vec<Address>) -> Result<Vec<Address>> {
        self.tracker.clone().register(self, addresses)
    }

    /// Unregisters a vector of addresses, removing them if they exist.
    ///
    /// Returns the addresses that where successfully removed.
    pub fn unregister(&mut self, addresses: Vec<Address>) -> Vec<Address> {
        self.tracker.clone().unregister(self, addresses)
    }

    /// Unregisters all indexes, draining the set in the process.
    pub fn clear(&mut self) {
        self.tracker.clone().clear_indexes(self)
    }

    pub fn iter(&self) -> hash_set::Iter<'_, Index> {
        self.indexes.iter()
    }

    pub fn len(&self) -> usize {
        self.indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.indexes.capacity()
    }

    #[cfg(test)]
    pub fn test_direct_insert(&mut self, index: Index) -> bool {
        self.insert(index)
    }
}

impl Indexer for IndexSet {
    fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    fn contains(&self, index: Index) -> bool {
        self.indexes.contains(&index)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl IndexerStorage for IndexSet {
    fn insert(&mut self, index: Index) -> bool {
        self.indexes.insert(index)
    }

    fn remove(&mut self, index: Index) -> bool {
        self.indexes.remove(&index)
    }
}

impl Clone for IndexSet {
    fn clone(&self) -> Self {
        // Clone the indexes...
        let indexes = self.indexes.clone();
        // ...and make sure that the tracker increases the reference count of each cloned index
        self.tracker.reference_indexes(indexes.iter());

        Self { tracker: self.tracker.clone(), indexes }
    }
}

impl Drop for IndexSet {
    fn drop(&mut self) {
        // Decreases the tracker indexes reference count
        self.clear()
    }
}

impl PartialEq for IndexSet {
    fn eq(&self, other: &Self) -> bool {
        self.indexes == other.indexes
    }
}
impl Eq for IndexSet {}

#[derive(Debug)]
struct Inner {
    /// Index-based map of [`ScriptPublicKey`] to its reference count
    ///
    /// ### Implementation note
    ///
    /// The whole purpose of the tracker is to reduce a [`ScriptPublicKey`] to an [`Index`] in all
    /// [`Indexer`] instances. Therefore, every mutable access to the struct must be careful not to
    /// use `IndexMap` APIs which alter the index order of existing entries.
    script_pub_keys: IndexMap<ScriptPublicKey, RefCount>,

    /// Maximum address count that can be registered
    max_addresses: Option<usize>,

    /// Set of entries [`Index`] in `script_pub_keys` having their [`RefCount`] at 0 hence considered
    /// empty.
    ///
    /// An empty entry can be recycled and hold a new `script_pub_key`.
    empty_entries: HashSet<Index>,
}

impl Inner {
    /// The upper bound of the maximum address count
    const MAX_ADDRESS_UPPER_BOUND: usize = Self::expand_max_addresses(10_000_000);

    /// The lower bound of the maximum address count
    const MAX_ADDRESS_LOWER_BOUND: usize = 6;

    /// Computes the optimal expanded max address count fitting in the actual allocated size of
    /// the internal storage structure
    const fn expand_max_addresses(max_addresses: usize) -> usize {
        if max_addresses >= Self::MAX_ADDRESS_LOWER_BOUND {
            // The following formula matches the internal allocation of an IndexMap or a HashMap
            // as found in fns hashbrown::raw::inner::{capacity_to_buckets, bucket_mask_to_capacity}.
            //
            // The last allocated entry is reserved for recycling entries, hence the plus and minus 1
            // which differ from the hashbrown formula.
            ((max_addresses + 1) * 8 / 7).next_power_of_two() * 7 / 8 - 1
        } else {
            Self::MAX_ADDRESS_LOWER_BOUND
        }
    }

    fn new(max_addresses: Option<usize>) -> Self {
        // Expands the maximum address count to the IndexMap actual usable allocated size minus 1.
        // Saving one entry for the insert/swap_remove scheme during entry recycling prevents a reallocation
        // when reaching the maximum.
        let max_addresses = max_addresses.map(Self::expand_max_addresses);
        let capacity = max_addresses.map(|x| x + 1).unwrap_or_default();

        assert!(
            capacity <= Self::MAX_ADDRESS_UPPER_BOUND + 1,
            "Tracker maximum address count cannot exceed {}",
            Self::MAX_ADDRESS_UPPER_BOUND
        );

        let script_pub_keys = IndexMap::with_capacity(capacity);
        debug!("Creating an address tracker with a capacity of {}", script_pub_keys.capacity());
        if let Some(max_addresses) = max_addresses {
            info!("Tracking UTXO changed events for {} addresses at most", max_addresses);
        }
        let empty_entries = HashSet::with_capacity(capacity);
        Self { script_pub_keys, max_addresses, empty_entries }
    }

    fn is_full(&self) -> bool {
        self.script_pub_keys.len() >= self.max_addresses.unwrap_or(Self::MAX_ADDRESS_UPPER_BOUND) && self.empty_entries.is_empty()
    }

    fn get(&self, spk: &ScriptPublicKey) -> Option<(Index, RefCount)> {
        self.script_pub_keys.get_full(spk).map(|(index, _, count)| (index as Index, *count))
    }

    fn get_index(&self, index: Index) -> Option<&ScriptPublicKey> {
        self.script_pub_keys.get_index(index as usize).map(|(spk, _)| spk)
    }

    fn get_index_address(&self, index: Index, prefix: Prefix) -> Option<Address> {
        self.script_pub_keys
            .get_index(index as usize)
            .map(|(spk, _)| extract_script_pub_key_address(spk, prefix).expect("is retro-convertible"))
    }

    fn get_or_insert(&mut self, spk: ScriptPublicKey) -> Result<Index> {
        match self.is_full() {
            false => match self.script_pub_keys.entry(spk) {
                Entry::Occupied(entry) => Ok(entry.index() as Index),
                Entry::Vacant(entry) => {
                    let mut index = entry.index() as Index;
                    trace!(
                        "AddressTracker insert #{} {}",
                        index,
                        extract_script_pub_key_address(entry.key(), Prefix::Mainnet).unwrap()
                    );
                    let _ = *entry.insert(0);

                    // Try to recycle an empty entry if there is some
                    let mut recycled = false;
                    if (index + 1) as usize == self.script_pub_keys.len() && !self.empty_entries.is_empty() {
                        // Takes the first empty entry index
                        let empty_index = self.empty_entries.iter().cloned().next();
                        if let Some(empty_index) = empty_index {
                            // Stores the newly created entry at the empty entry index while keeping it registered as an
                            // empty entry (because it is so at this stage, the ref count being 0).
                            self.script_pub_keys.swap_remove_index(empty_index as usize);
                            index = empty_index;
                            recycled = true;
                        }
                    }
                    // If no recycling occurred, registers the newly created entry as empty (since ref count is 0).
                    if !recycled {
                        self.empty_entries.insert(index);
                    }
                    Ok(index)
                }
            },
            true => match self.script_pub_keys.get_index_of(&spk) {
                Some(index) => Ok(index as Index),
                None => Err(Error::MaxCapacityReached),
            },
        }
    }

    /// Increases by one the [`RefCount`] of the [`ScriptPublicKey`] at `index`.
    ///
    /// If the entry had a reference count of 0 before the increase, its index is removed from
    /// the empty entries set.
    fn inc_count(&mut self, index: Index) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(index as usize) {
            *count += 1;
            trace!("AddressTracker inc count #{} to {}", index, *count);
            if *count == 1 {
                self.empty_entries.remove(&index);
            }
        }
    }

    /// Decreases by one the [`RefCount`] of the [`ScriptPublicKey`] at `index`.
    ///
    /// Panics if the ref count is already 0.
    ///
    /// When the reference count reaches zero, the index is inserted into the empty entries set.
    fn dec_count(&mut self, index: Index) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(index as usize) {
            if *count == 0 {
                panic!("Address tracker is trying to decrease an address counter that is already at zero");
            }
            *count -= 1;
            trace!("AddressTracker dec count #{} to {}", index, *count);
            if *count == 0 {
                self.empty_entries.insert(index);
            }
        }
    }

    fn len(&self) -> usize {
        assert!(self.script_pub_keys.len() >= self.empty_entries.len(), "entries marked empty are never removed from script_pub_keys");
        self.script_pub_keys.len() - self.empty_entries.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Tracker of a set of [`Address`](kaspa_addresses::Address), indexing and counting registrations
///
/// #### Implementation design
///
/// Each [`Address`](kaspa_addresses::Address) is stored internally as a [`ScriptPubKey`](kaspa_consensus_core::tx::ScriptPublicKey).
/// This prevents inter-network duplication and optimizes UTXOs filtering efficiency.
///
/// But consequently the address network prefix gets lost and must be globally provided when querying for addresses by indexes.
#[derive(Debug, Clone)]
pub struct Tracker {
    inner: Arc<RwLock<Inner>>,
}

impl Display for Tracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} addresses", self.inner.read().script_pub_keys.len())
    }
}

impl Tracker {
    /// The upper bound of the maximum address count
    pub const MAX_ADDRESS_UPPER_BOUND: usize = Inner::MAX_ADDRESS_UPPER_BOUND;

    /// Expanded count for a maximum of 1M addresses
    pub const DEFAULT_MAX_ADDRESSES: usize = Self::expand_max_addresses(800);

    const ADDRESS_CHUNK_SIZE: usize = 1024;

    /// Computes the optimal expanded max address count fitting in the actual allocated size of
    /// the internal storage structure
    pub const fn expand_max_addresses(max_addresses: usize) -> usize {
        Inner::expand_max_addresses(max_addresses)
    }

    pub fn new(max_addresses: Option<usize>) -> Self {
        Self { inner: Arc::new(RwLock::new(Inner::new(max_addresses))) }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let tracker = Self { inner: Arc::new(RwLock::new(Inner::new(None))) };
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = tracker.inner.write();
            for address in chunk {
                let index = inner.get_or_insert(pay_to_address_script(address)).unwrap();
                inner.inc_count(index);
            }
        }
        tracker
    }

    pub fn data(&self) -> TrackerReadGuard<'_> {
        TrackerReadGuard { guard: self.inner.read() }
    }

    fn get(&self, spk: &ScriptPublicKey) -> Option<(Index, RefCount)> {
        self.inner.read().get(spk)
    }

    #[cfg(test)]
    fn get_address(&self, address: &Address) -> Option<(Index, RefCount)> {
        self.get(&pay_to_address_script(address))
    }

    pub fn get_address_at_index(&self, index: Index, prefix: Prefix) -> Option<Address> {
        self.inner.read().get_index_address(index, prefix)
    }

    pub fn contains<T: Indexer>(&self, indexes: &T, spk: &ScriptPublicKey) -> bool {
        self.get(spk).is_some_and(|(index, _)| indexes.contains(index))
    }

    pub fn contains_address<T: Indexer>(&self, indexes: &T, address: &Address) -> bool {
        self.contains(indexes, &pay_to_address_script(address))
    }

    /// Tries to register an `Address` vector into an `Indexer`. The addresses are first registered in the tracker if unknown
    /// yet and their reference count is increased when successfully inserted in the `Indexer`.
    ///
    /// On success, returns the addresses that were actually inserted in the `Indexer`.
    ///
    /// Fails if the maximum capacity gets reached, leaving the tracker unchanged.
    fn register<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Result<Vec<Address>> {
        let mut rollback: bool = false;
        {
            let mut counter: usize = 0;
            let mut inner = self.inner.write();
            addresses.retain(|address| {
                counter += 1;
                if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                    RwLockWriteGuard::bump(&mut inner);
                }
                let spk = pay_to_address_script(address);
                match inner.get_or_insert(spk) {
                    Ok(index) => {
                        if indexes.insert(index) {
                            inner.inc_count(index);
                            true
                        } else {
                            false
                        }
                    }
                    Err(Error::MaxCapacityReached) => {
                        // Rollback registration
                        rollback = true;
                        false
                    }
                }
            });
        }
        match rollback {
            false => Ok(addresses),
            true => {
                let _ = self.unregister(indexes, addresses);
                Err(Error::MaxCapacityReached)
            }
        }
    }

    /// Unregisters an `Address` vector from an `Indexer`. The addresses, when existing both in the tracker
    /// and the `Indexer`, are first removed from the `Indexer` and on success get their reference count
    /// decreased.
    ///
    /// Returns the addresses that where successfully unregistered from the `Indexer`.
    fn unregister<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Vec<Address> {
        if indexes.is_empty() {
            vec![]
        } else {
            let mut counter: usize = 0;
            let mut inner = self.inner.write();
            addresses.retain(|address| {
                counter += 1;
                if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                    RwLockWriteGuard::bump(&mut inner);
                }
                let spk = pay_to_address_script(address);
                if let Some((index, _)) = inner.get(&spk) {
                    if indexes.remove(index) {
                        inner.dec_count(index);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            addresses
        }
    }

    fn reference_indexes<'a>(&self, indexes: impl Iterator<Item = &'a Index>) {
        for chunk in &indexes.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|index| inner.inc_count(*index));
        }
    }

    /// Unregisters all indexes contained in `indexes`, draining it in the process.
    fn clear_indexes(&self, indexes: &mut Indexes) {
        for chunk in &indexes.indexes.drain().chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|index| inner.dec_count(index));
        }
    }

    /// Unregisters all indexes contained in `counters`, draining it in the process.
    fn clear_counters(&self, counters: &mut Counters) {
        for chunk in &counters.indexes.drain().chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|(index, counter)| {
                if counter > 0 {
                    inner.dec_count(index)
                }
            });
        }
    }

    pub fn to_addresses(&self, indexes: &[Index], prefix: Prefix) -> Vec<Address> {
        let mut addresses = Vec::with_capacity(indexes.len());
        for chunk in indexes.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let inner = self.inner.read();
            chunk.iter().for_each(|index| {
                if let Some(address) = inner.get_index_address(*index, prefix) {
                    addresses.push(address);
                }
            });
        }
        addresses
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.inner.read().script_pub_keys.capacity()
    }

    pub fn max_addresses(&self) -> Option<usize> {
        self.inner.read().max_addresses
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new(None)
    }
}

pub struct TrackerReadGuard<'a> {
    guard: RwLockReadGuard<'a, Inner>,
}

impl<'a> TrackerReadGuard<'a> {
    pub fn get_index(&'a self, index: Index) -> Option<&'a ScriptPublicKey> {
        self.guard.get_index(index)
    }

    pub fn iter_keys(&'a self, indexes: &'a Indexes) -> impl Iterator<Item = Option<&'a ScriptPublicKey>> {
        indexes.indexes.iter().cloned().map(|index| self.get_index(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_math::Uint256;

    fn create_addresses(start: usize, count: usize) -> Vec<Address> {
        (start..start + count)
            .map(|i| Address::new(Prefix::Mainnet, kaspa_addresses::Version::PubKey, &Uint256::from_u64(i as u64).to_le_bytes()))
            .collect()
    }

    #[test]
    fn test_tracker_capacity_and_entry_recycling() {
        const INIT_MAX_ADDRESSES: usize = 6;
        const MAX_ADDRESSES: usize = ((INIT_MAX_ADDRESSES + 1) * 8 / 7).next_power_of_two() * 7 / 8 - 1;
        const CAPACITY: usize = MAX_ADDRESSES + 1;

        let tracker = Tracker::new(Some(MAX_ADDRESSES));
        assert_eq!(
            tracker.max_addresses().unwrap(),
            MAX_ADDRESSES,
            "tracker maximum address count should be expanded to the available allocated entries, minus 1 for a transient insert/swap_remove"
        );
        assert_eq!(
            tracker.capacity(),
            CAPACITY,
            "tracker capacity should match the maximum address count plus 1 extra entry for a transient insert/swap_remove"
        );
        let aa = create_addresses(0, MAX_ADDRESSES);
        assert_eq!(aa.len(), MAX_ADDRESSES);

        // Register addresses 0..MAX_ADDRESSES
        let mut idx_a = Indexes::new(tracker.clone(), vec![]);
        let aa = idx_a.register(aa).unwrap();
        let aai = aa.iter().map(|x| tracker.get_address(x).unwrap().0).collect_vec();
        assert_eq!(aa.len(), MAX_ADDRESSES, "all addresses should be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "all addresses should be registered");
        for i in 0..aa.len() {
            assert!(tracker.contains_address(&idx_a, &aa[i]), "tracker should contain the registered address");
            assert!(idx_a.contains(aai[i]), "index set should contain the registered address index");
        }
        assert_eq!(tracker.capacity(), CAPACITY);

        // Try to re-register addresses 0..MAX_ADDRESSES
        let a = idx_a.register(aa).unwrap();
        assert_eq!(a.len(), 0, "all addresses should already be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "all addresses should still be registered");

        // Try to register an additional address while the tracker is full
        assert!(idx_a.register(create_addresses(MAX_ADDRESSES, 1)).is_err(), "the tracker is full and should refuse a new address");

        // Register address set 1..MAX_ADDRESSES, already fully covered by the tracker address set
        const AB_COUNT: usize = MAX_ADDRESSES - 1;
        let mut idx_b = Indexes::new(tracker.clone(), vec![]);
        let ab = idx_b.register(create_addresses(1, AB_COUNT)).unwrap();
        assert_eq!(ab.len(), AB_COUNT, "all addresses should be registered");
        assert_eq!(idx_b.len(), AB_COUNT, "all addresses should be registered");

        // Empty the tracker entry containing A0
        assert_eq!(idx_a.unregister(create_addresses(0, 1)).len(), 1);
        assert_eq!(idx_a.len(), MAX_ADDRESSES - 1, "entry #0 with address A0 should now be marked empty");

        // Fill the empty entry with a single new address A8
        const AC_COUNT: usize = 1;
        let ac = idx_a.register(create_addresses(MAX_ADDRESSES, AC_COUNT)).unwrap();
        let aci = ac.iter().map(|x| tracker.get_address(x).unwrap().0).collect_vec();
        assert_eq!(ac.len(), AC_COUNT, "a new address should be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "a new address should be registered");
        assert_eq!(ac[0], create_addresses(MAX_ADDRESSES, AC_COUNT)[0], "the new address A8 should be registered");
        assert!(tracker.contains_address(&idx_a, &ac[0]), "the new address A8 should be registered");
        assert_eq!(aai[0], aci[0], "the newly registered address A8 should occupy the previously emptied entry");

        assert_eq!(
            tracker.capacity(),
            CAPACITY,
            "the tracker capacity should not have been affected by the transient insert/swap_remove"
        );
    }

    #[test]
    fn test_indexes_eq() {
        let tracker = Tracker::new(None);
        let i1 = IndexSet::new(tracker.clone(), vec![0, 1, 2, 3, 5, 7, 11]);
        let i2 = IndexSet::new(tracker.clone(), vec![5, 7, 11, 0, 1, 2, 3]);
        let i3 = IndexSet::new(tracker.clone(), vec![0, 1, 2, 4, 8, 16, 32]);
        let i4 = IndexSet::new(tracker.clone(), vec![0, 1]);
        assert_eq!(i1, i1);
        assert_eq!(i1, i2);
        assert_ne!(i1, i3);
        assert_ne!(i1, i4);
        assert_eq!(i2, i2);
        assert_ne!(i2, i3);
        assert_ne!(i2, i4);
        assert_eq!(i3, i3);
        assert_ne!(i3, i4);
        assert_eq!(i4, i4);
    }

    #[test]
    fn test_index_map_replace() {
        let mut m: IndexMap<u64, RefCount> = IndexMap::with_capacity(7);
        m.insert(1, 10);
        m.insert(2, 0);
        m.insert(3, 30);
        m.insert(4, 40);
        assert_eq!(m.get_index(0), Some((&1, &10)));
        assert_eq!(m.get_index(1), Some((&2, &0)));
        assert_eq!(m.get_index(2), Some((&3, &30)));
        assert_eq!(m.get_index(3), Some((&4, &40)));

        assert_eq!(m.swap_remove_index(1), Some((2, 0)));

        assert_eq!(m.get_index(0), Some((&1, &10)));
        assert_eq!(m.get_index(1), Some((&4, &40)));
        assert_eq!(m.get_index(2), Some((&3, &30)));
    }

    #[test]
    fn test_index_map_capacity() {
        const CAPACITY: usize = 14;
        let mut m: IndexMap<u64, RefCount> = IndexMap::with_capacity(CAPACITY);
        for i in 0..CAPACITY {
            m.insert(i as u64, 0);
            assert_eq!(m.capacity(), CAPACITY);
        }
        m.insert(CAPACITY as u64 + 1, 0);
        assert_eq!(m.capacity(), ((CAPACITY + 1) * 8 / 7).next_power_of_two() * 7 / 8);
    }
}
