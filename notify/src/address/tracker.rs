use crate::address::error::{Error, Result};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::{debug, trace};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    collections::{hash_map, hash_set, HashMap, HashSet},
    fmt::Display,
};

pub trait Indexer {
    fn contains(&self, index: Index) -> bool;

    /// Inserts an [`Index`].
    ///
    /// Returns true if the index was not present and was successfully inserted, false otherwise.
    fn insert(&mut self, index: Index) -> bool;

    /// Removes an [`Index`].
    ///
    /// Returns true if the index was present and successfully removed, false otherwise.
    fn remove(&mut self, index: Index) -> bool;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

pub type Index = u32;
pub type RefCount = u16;

/// Tracks reference count of indexes
pub type Counters = CounterMap;

/// Tracks indexes
pub type Indexes = IndexSet;

/// Tracks reference count of indexes
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CounterMap(HashMap<Index, RefCount>);

impl CounterMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    #[cfg(test)]
    pub fn with_counters(counters: Vec<Counter>) -> Self {
        Self(counters.into_iter().map(|x| (x.index, x.count)).collect())
    }

    pub fn iter(&self) -> hash_map::Iter<'_, Index, RefCount> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl Indexer for CounterMap {
    fn contains(&self, index: Index) -> bool {
        self.0.contains_key(&index)
    }

    fn insert(&mut self, index: Index) -> bool {
        let mut result = true;
        self.0
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
        self.0.entry(index).and_modify(|x| {
            if *x > 0 {
                *x -= 1;
                result = *x == 0
            }
        });
        result
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
#[cfg(test)]
pub struct Counter {
    pub index: Index,
    pub count: RefCount,
    pub locked: bool,
}

#[cfg(test)]
impl Counter {
    pub fn new(index: Index, count: RefCount) -> Self {
        Self { index, count, locked: false }
    }

    pub fn active(&self) -> bool {
        self.count > 0
    }
}

#[cfg(test)]
impl PartialEq for Counter {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
#[cfg(test)]
impl Eq for Counter {}

#[cfg(test)]
impl PartialOrd for Counter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[cfg(test)]
impl Ord for Counter {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

/// Set of `Index`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSet(HashSet<Index>);

impl IndexSet {
    pub fn new(indexes: Vec<Index>) -> Self {
        Self(indexes.into_iter().collect())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashSet::with_capacity(capacity))
    }

    pub fn iter(&self) -> hash_set::Iter<'_, Index> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn drain(&mut self) -> hash_set::Drain<'_, Index> {
        self.0.drain()
    }
}

impl Indexer for IndexSet {
    fn contains(&self, index: Index) -> bool {
        self.0.contains(&index)
    }

    fn insert(&mut self, index: Index) -> bool {
        self.0.insert(index)
    }

    fn remove(&mut self, index: Index) -> bool {
        self.0.remove(&index)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

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

    /// Maximum address count that can be registered. Note this must be `<= Index::MAX` since we cast the returned indexes to `Index`
    max_addresses: usize,

    /// The preallocation used for the address index (`script_pub_keys`)
    addresses_preallocation: Option<usize>,

    /// Set of entries [`Index`] in `script_pub_keys` having their [`RefCount`] at 0 hence considered
    /// empty.
    ///
    /// An empty entry can be recycled and hold a new `script_pub_key`.
    empty_entries: HashSet<Index>,
}

/// Fails at compile time if `MAX_ADDRESS_UPPER_BOUND > Index::MAX`.
/// This is mandatory since we cast the returned indexes to `Index`
const _: usize = Index::MAX as usize - Inner::MAX_ADDRESS_UPPER_BOUND;

impl Inner {
    /// The upper bound of the maximum address count. Note that the upper bound must
    /// never exceed `Index::MAX` since we cast the returned indexes to `Index`. See
    /// compile-time assertion above
    const MAX_ADDRESS_UPPER_BOUND: usize = Self::expand_max_addresses(10_000_000);

    /// The lower bound of the maximum address count
    const MAX_ADDRESS_LOWER_BOUND: usize = 6;

    /// Expanded count for a maximum of 1M addresses
    const DEFAULT_MAX_ADDRESSES: usize = Self::expand_max_addresses(1_000_000);

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
        let addresses_preallocation = max_addresses;
        let capacity = max_addresses.map(|x| x + 1).unwrap_or_default();

        assert!(
            capacity <= Self::MAX_ADDRESS_UPPER_BOUND + 1,
            "Tracker maximum address count cannot exceed {}",
            Self::MAX_ADDRESS_UPPER_BOUND
        );
        let max_addresses = max_addresses.unwrap_or(Self::DEFAULT_MAX_ADDRESSES);
        debug!("Memory configuration: UTXO changed events wil be tracked for at most {} addresses", max_addresses);

        let script_pub_keys = IndexMap::with_capacity(capacity);
        debug!("Creating an address tracker with a capacity of {}", script_pub_keys.capacity());

        let empty_entries = HashSet::with_capacity(capacity);
        Self { script_pub_keys, max_addresses, addresses_preallocation, empty_entries }
    }

    fn is_full(&self) -> bool {
        self.script_pub_keys.len() >= self.max_addresses && self.empty_entries.is_empty()
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
#[derive(Debug)]
pub struct Tracker {
    inner: RwLock<Inner>,
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
    pub const DEFAULT_MAX_ADDRESSES: usize = Inner::DEFAULT_MAX_ADDRESSES;

    const ADDRESS_CHUNK_SIZE: usize = 1024;

    /// Computes the optimal expanded max address count fitting in the actual allocated size of
    /// the internal storage structure
    pub const fn expand_max_addresses(max_addresses: usize) -> usize {
        Inner::expand_max_addresses(max_addresses)
    }

    /// Creates a new `Tracker` instance. If `max_addresses` is `Some`, uses it to prealloc
    /// the internal index as well as for bounding the index size. Otherwise, performs no
    /// prealloc while bounding the index size by `Tracker::DEFAULT_MAX_ADDRESSES`
    pub fn new(max_addresses: Option<usize>) -> Self {
        Self { inner: RwLock::new(Inner::new(max_addresses)) }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let tracker = Self { inner: RwLock::new(Inner::new(None)) };
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

    pub fn get(&self, spk: &ScriptPublicKey) -> Option<(Index, RefCount)> {
        self.inner.read().get(spk)
    }

    pub fn get_address(&self, address: &Address) -> Option<(Index, RefCount)> {
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

    /// Returns an index set containing the indexes of all the addresses both registered in the tracker and in `indexes`.
    pub fn unregistering_indexes(&self, indexes: &Indexes, addresses: &[Address]) -> Indexes {
        Indexes::new(
            addresses
                .iter()
                .filter_map(|address| {
                    self.get(&pay_to_address_script(address)).and_then(|(index, _)| indexes.contains(index).then_some(index))
                })
                .collect(),
        )
    }

    /// Tries to register an `Address` vector into an `Indexer`. The addresses are first registered in the tracker if unknown
    /// yet and their reference count is increased when successfully inserted in the `Indexer`.
    ///
    /// On success, returns the addresses that were actually inserted in the `Indexer`.
    ///
    /// Fails if the maximum capacity gets reached, leaving the tracker unchanged.
    pub fn register<T: Indexer>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Result<Vec<Address>> {
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
    pub fn unregister<T: Indexer>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Vec<Address> {
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

    /// Unregisters all indexes contained in `indexes`, draining it in the process.
    pub fn unregister_indexes(&self, indexes: &mut Indexes) {
        for chunk in &indexes.drain().chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|index| inner.dec_count(index));
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

    pub fn addresses_preallocation(&self) -> Option<usize> {
        self.inner.read().addresses_preallocation
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
        indexes.0.iter().cloned().map(|index| self.get_index(index))
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
            tracker.addresses_preallocation().unwrap(),
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
        let mut idx_a = Indexes::new(vec![]);
        let aa = tracker.register(&mut idx_a, aa).unwrap();
        let aai = aa.iter().map(|x| tracker.get_address(x).unwrap().0).collect_vec();
        assert_eq!(aa.len(), MAX_ADDRESSES, "all addresses should be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "all addresses should be registered");
        for i in 0..aa.len() {
            assert!(tracker.contains_address(&idx_a, &aa[i]), "tracker should contain the registered address");
            assert!(idx_a.contains(aai[i]), "index set should contain the registered address index");
        }
        assert_eq!(tracker.capacity(), CAPACITY);

        // Try to re-register addresses 0..MAX_ADDRESSES
        let a = tracker.register(&mut idx_a, aa).unwrap();
        assert_eq!(a.len(), 0, "all addresses should already be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "all addresses should still be registered");

        // Try to register an additional address while the tracker is full
        assert!(
            tracker.register(&mut idx_a, create_addresses(MAX_ADDRESSES, 1)).is_err(),
            "the tracker is full and should refuse a new address"
        );

        // Register address set 1..MAX_ADDRESSES, already fully covered by the tracker address set
        const AB_COUNT: usize = MAX_ADDRESSES - 1;
        let mut idx_b = Indexes::new(vec![]);
        let ab = tracker.register(&mut idx_b, create_addresses(1, AB_COUNT)).unwrap();
        assert_eq!(ab.len(), AB_COUNT, "all addresses should be registered");
        assert_eq!(idx_b.len(), AB_COUNT, "all addresses should be registered");

        // Empty the tracker entry containing A0
        assert_eq!(tracker.unregister(&mut idx_a, create_addresses(0, 1)).len(), 1);
        assert_eq!(idx_a.len(), MAX_ADDRESSES - 1, "entry #0 with address A0 should now be marked empty");

        // Fill the empty entry with a single new address A8
        const AC_COUNT: usize = 1;
        let ac = tracker.register(&mut idx_a, create_addresses(MAX_ADDRESSES, AC_COUNT)).unwrap();
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
        let i1 = IndexSet::new(vec![0, 1, 2, 3, 5, 7, 11]);
        let i2 = IndexSet::new(vec![5, 7, 11, 0, 1, 2, 3]);
        let i3 = IndexSet::new(vec![0, 1, 2, 4, 8, 16, 32]);
        let i4 = IndexSet::new(vec![0, 1]);
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
