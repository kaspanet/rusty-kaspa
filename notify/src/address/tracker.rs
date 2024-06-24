use crate::address::error::{Error, Result};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::{debug, trace};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

pub trait Indexer {
    /// The tracker used internally to register/unregister/lookup addresses
    fn tracker(&self) -> &Tracker;

    /// Returns `true` if the `Indexer` contains the index.
    fn contains_index(&self, index: Index) -> bool;

    /// Returns `true` if the `Indexer` contains the value.
    fn contains_spk(&self, spk: &ScriptPublicKey) -> bool {
        self.tracker().get(spk).is_some_and(|(index, counter)| (counter > 0) && self.contains_index(index))
    }

    /// Returns `true` if the `Indexer` contains the value.
    fn contains(&self, address: &Address) -> bool {
        self.contains_spk(&pay_to_address_script(address))
    }

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;

    fn iter_index(&self) -> impl Iterator<Item = Index>;
    fn iter_address(&self) -> impl Iterator<Item = Address> {
        self.iter_index()
            .map(|index| self.tracker().get_index_address(index).expect("the index is tracked thus has a matching address"))
    }
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

    /// Number of empty entries
    empty_entries: usize,
}

impl CounterMap {
    pub fn new(tracker: Tracker) -> Self {
        Self { tracker, indexes: HashMap::new(), empty_entries: 0 }
    }

    pub fn with_capacity(tracker: Tracker, capacity: usize) -> Self {
        Self { tracker, indexes: HashMap::with_capacity(capacity), empty_entries: 0 }
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
    pub fn unregister(&mut self, addresses: Vec<Address>) -> Result<Vec<Address>> {
        self.tracker.clone().unregister(self, addresses)
    }

    /// Registers the indexes contained in an [`Indexes`].
    ///
    /// Returns the indexes that were actually inserted.
    pub fn register_indexes(&mut self, indexes: Indexes) -> Indexes {
        self.tracker.clone().register_indexes(self, indexes)
    }

    /// Unregisters the indexes contained in an [`Indexes`].
    ///
    /// Returns the indexes that were successfully removed.
    pub fn unregister_indexes(&mut self, indexes: Indexes) -> Indexes {
        self.tracker.clone().unregister_indexes(self, indexes)
    }

    /// Returns an [`Indexes`] containing a copy of all active indexes.
    pub fn get_indexes(&self) -> Indexes {
        // Create the destination index set with enough capacity to store all active indexes
        let mut target = Indexes::with_capacity(self.tracker.clone(), self.len());
        // Fill the index set with copies of all indexes having a reference count greater than zero
        self.indexes.iter().for_each(|(index, count)| {
            if *count > 0 {
                let _ = target.insert(*index);
            }
        });
        // Reference all duplicated indexes in the tracker
        self.tracker.reference_indexes(target.iter_index());
        target
    }

    /// Converts the indexes into addresses.
    pub fn to_addresses(&self) -> Vec<Address> {
        self.tracker().to_addresses(self)
    }

    /// Unregisters all indexes, draining the map in the process.
    pub fn clear(&mut self) {
        self.tracker.clone().clear_counters(self)
    }

    pub fn len(&self) -> usize {
        self.indexes.len() - self.empty_entries
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty() || self.indexes.len() == self.empty_entries
    }

    pub fn capacity(&self) -> usize {
        self.indexes.capacity()
    }

    #[cfg(test)]
    pub fn test_direct_insert(&mut self, index: Index) -> bool {
        self.insert(index)
    }

    #[cfg(test)]
    fn test_get_address(&self, address: &Address) -> Option<RefCount> {
        self.tracker.get_address(address).and_then(|(index, _)| self.indexes.get(&index).copied())
    }
}

impl Indexer for CounterMap {
    fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    fn contains_index(&self, index: Index) -> bool {
        self.indexes.get(&index).is_some_and(|counter| *counter > 0)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn iter_index(&self) -> impl Iterator<Item = Index> {
        self.indexes.keys().copied()
    }
}

impl IndexerStorage for CounterMap {
    fn insert(&mut self, index: Index) -> bool {
        let mut result = true;
        self.indexes
            .entry(index)
            .and_modify(|x| {
                *x += 1;
                if *x == 1 {
                    self.empty_entries -= 1;
                } else {
                    result = false;
                }
            })
            .or_insert(1);
        result
    }

    fn remove(&mut self, index: Index) -> bool {
        let mut result = false;
        self.indexes.entry(index).and_modify(|x| {
            if *x > 0 {
                *x -= 1;
                if *x == 0 {
                    self.empty_entries += 1;
                    result = true;
                }
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
        // which associated ref counter here is non-zero
        self.tracker.reference_indexes(indexes.iter().filter_map(|(index, count)| (*count > 0).then_some(*index)));

        Self { tracker: self.tracker.clone(), indexes, empty_entries: self.empty_entries }
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
    pub fn new(tracker: Tracker) -> Self {
        Self { tracker, indexes: HashSet::new() }
    }

    pub fn with_capacity(tracker: Tracker, capacity: usize) -> Self {
        Self { tracker, indexes: HashSet::with_capacity(capacity) }
    }

    /// Registers a vector of addresses, inserting them if not existing yet.
    ///
    /// On success, returns the addresses that were actually inserted.
    ///
    /// Fails, leaving both the tracker and the object unchanged, if:
    ///
    /// - the tracker does not have `Some` prefix, returning [`Error::NoPrefix`],
    /// - the maximum capacity of the underlying [`Tracker`] gets reached, returning [`Error::MaxCapacityReached`],
    /// - some address did not match the tracker prefix, returning [`Error::PrefixMismatch`] with the offending address.
    pub fn register(&mut self, addresses: Vec<Address>) -> Result<Vec<Address>> {
        self.tracker.clone().register(self, addresses)
    }

    /// Unregisters a vector of addresses, removing them if they exist.
    ///
    /// On success, returns the addresses that where successfully removed.
    ///
    /// Fails, leaving both the tracker and the object unchanged, if:
    ///
    /// - the tracker does not have `Some` prefix, returning [`Error::NoPrefix`],
    /// - some address did not match the tracker prefix, returning [`Error::PrefixMismatch`] with the offending address.
    pub fn unregister(&mut self, addresses: Vec<Address>) -> Result<Vec<Address>> {
        self.tracker.clone().unregister(self, addresses)
    }

    /// Registers the indexes contained in an [`Indexes`].
    ///
    /// Returns the indexes that were actually inserted.
    pub fn register_indexes(&mut self, indexes: Indexes) -> Self {
        self.tracker.clone().register_indexes(self, indexes)
    }

    /// Unregisters the indexes contained in an [`Indexes`].
    ///
    /// Returns the indexes that were successfully removed.
    pub fn unregister_indexes(&mut self, indexes: Indexes) -> Self {
        self.tracker.clone().unregister_indexes(self, indexes)
    }

    /// Drains all indexes, inserts them in a new instance and returns it.
    ///
    /// Keeps the allocated memory for reuse.
    pub fn transfer(&mut self) -> Self {
        // This operation has no impact on the reference count of the indexes in the tracker
        // since indexes are moved from `self` to `target`.
        let mut target = Self::with_capacity(self.tracker.clone(), self.len());
        self.indexes.drain().for_each(|index| {
            let _ = target.indexes.insert(index);
        });
        target
    }

    /// Converts the indexes into addresses.
    pub fn to_addresses(&self) -> Vec<Address> {
        self.tracker().to_addresses(self)
    }

    /// Unregisters all indexes, draining the set in the process.
    pub fn clear(&mut self) {
        self.tracker.clone().clear_indexes(self)
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
    pub fn test_with_indexes(tracker: Tracker, indexes: Vec<Index>) -> Self {
        tracker.reference_indexes(indexes.iter().copied());
        Self { tracker, indexes: indexes.into_iter().collect() }
    }

    #[cfg(test)]
    pub fn test_direct_insert(&mut self, index: Index) -> bool {
        self.insert(index)
    }

    #[cfg(test)]
    fn test_get_address(&self, address: &Address) -> bool {
        self.tracker.get_address(address).and_then(|(index, _)| self.indexes.get(&index)).is_some()
    }
}

impl Indexer for IndexSet {
    fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    fn contains_index(&self, index: Index) -> bool {
        self.indexes.contains(&index)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn iter_index(&self) -> impl Iterator<Item = Index> {
        self.indexes.iter().copied()
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
        self.tracker.reference_indexes(indexes.iter().copied());

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
    /// Addresses prefix
    prefix: Option<Prefix>,

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

    fn new(prefix: Option<Prefix>, max_addresses: Option<usize>) -> Self {
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
        Self { prefix, script_pub_keys, max_addresses, addresses_preallocation, empty_entries }
    }

    /// Tries to set the address prefix.
    ///
    /// For this to succeed, the tracker must have no prefix yet or `prefix` must match the internal prefix.
    fn set_prefix(&mut self, prefix: Prefix) -> bool {
        if self.prefix.is_none() {
            self.prefix = Some(prefix);
            true
        } else {
            self.prefix == Some(prefix)
        }
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

    fn get_index_address(&self, index: Index) -> Option<Address> {
        self.script_pub_keys.get_index(index as usize).map(|(spk, _)| {
            extract_script_pub_key_address(spk, self.prefix.expect("no address tracking without a defined prefix"))
                .expect("is retro-convertible")
        })
    }

    /// Get or insert `spk` and returns its index.
    ///
    /// Fails with [`Error::MaxCapacityReached`] if the tracker is full and `spk` should have been inserted.
    ///
    /// Note: on success, the returned index may point to an empty entry, notably in case of an insertion.
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
/// This optimizes UTXOs filtering efficiency, which is the most solicited service in the tracker.
///
/// The tracker needs a defined prefix in order to be active. This prefix can be provided to the ctor or via a call to `set_prefix()`.
/// Once defined, the tracker prefix cannot be changed anymore.
///
/// The tracker rejects addresses whose prefix differs from that of the tracker.
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
    pub const DEFAULT_MAX_ADDRESSES: usize = Inner::DEFAULT_MAX_ADDRESSES;

    const ADDRESS_CHUNK_SIZE: usize = 1024;

    /// Computes the optimal expanded max address count fitting in the actual allocated size of
    /// the internal storage structure
    pub const fn expand_max_addresses(max_addresses: usize) -> usize {
        Inner::expand_max_addresses(max_addresses)
    }

    /// Creates a new `Tracker` instance.
    ///
    /// If `prefix` is `Some`, the tracker is activated and tracks matching addresses. Otherwise, the tracker
    /// rejects all addresses until `set_prefix()` get called.
    ///
    /// If `max_addresses` is `Some`, uses it to prealloc the internal index as well as for bounding the index
    /// size. Otherwise, performs no prealloc while bounding the index size by `Tracker::DEFAULT_MAX_ADDRESSES`.
    pub fn new(prefix: Option<Prefix>, max_addresses: Option<usize>) -> Self {
        Self { inner: Arc::new(RwLock::new(Inner::new(prefix, max_addresses))) }
    }

    #[cfg(test)]
    pub fn with_addresses(prefix: Prefix, addresses: &[Address]) -> Self {
        let tracker = Self { inner: Arc::new(RwLock::new(Inner::new(Some(prefix), None))) };
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = tracker.inner.write();
            for address in chunk {
                let index = inner.get_or_insert(pay_to_address_script(address)).unwrap();
                inner.inc_count(index);
            }
        }
        tracker
    }

    pub fn prefix(&self) -> Option<Prefix> {
        self.inner.read().prefix
    }

    /// Tries to set the address prefix.
    ///
    /// For this to succeed, the tracker must have no prefix yet or `prefix` must match the internal prefix.
    pub fn set_prefix(&self, prefix: Prefix) -> bool {
        self.inner.write().set_prefix(prefix)
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

    fn get_index_address(&self, index: Index) -> Option<Address> {
        self.inner.read().get_index_address(index)
    }

    /// Tries to register an `Address` vector into an `Indexer`. The addresses are first registered in the tracker if unknown
    /// yet and their reference count is increased when successfully inserted in the `Indexer`.
    ///
    /// On success, returns the addresses that were actually inserted in the `Indexer`.
    ///
    /// Fails, leaving the tracker unchanged, if:
    ///
    /// - the tracker does not have `Some` prefix,
    /// - the maximum capacity gets reached,
    /// - some address did not match the tracker prefix.
    fn register<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Result<Vec<Address>> {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker().inner));

        let mut result: Result<()> = Ok(());
        {
            let mut inner = self.inner.write();
            if let Some(prefix) = inner.prefix {
                let mut counter: usize = 0;
                addresses.retain(|address| {
                    counter += 1;
                    if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                        RwLockWriteGuard::bump(&mut inner);
                    }
                    if result.is_err() {
                        false
                    } else if address.prefix == prefix {
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
                            Err(err) => {
                                result = Err(err);
                                false
                            }
                        }
                    } else {
                        result = Err(Error::PrefixMismatch(address.clone()));
                        false
                    }
                });
            } else {
                return Err(Error::NoPrefix);
            }
        }
        match result {
            Ok(()) => Ok(addresses),
            Err(err) => {
                // Rollback if some error occurred...
                let _ = self.unregister(indexes, addresses);
                // ...and return the error
                Err(err)
            }
        }
    }

    /// Registers the indexes contained in an [`Indexes`] into an [`Indexer`].
    ///
    /// Returns the indexes that were actually inserted in the [`Indexer`].
    fn register_indexes<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut other: Indexes) -> Indexes {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker().inner));
        assert!(Arc::ptr_eq(&self.inner, &other.tracker.inner));

        let mut counter: usize = 0;
        let mut inner = self.inner.write();
        other.indexes.retain(|index| {
            counter += 1;
            if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                RwLockWriteGuard::bump(&mut inner);
            }
            if indexes.insert(*index) {
                // Increase `index` ref count since `indexes` newly inserted the index and `other` retains it too
                inner.inc_count(*index);
                true
            } else {
                // Decrease `index` ref count since `indexes` already contains the index and `other` does not retain it
                inner.dec_count(*index);
                false
            }
        });
        other
    }

    /// Tries to unregisters an [`Address`] vector from an [`Indexer`]. The addresses, when existing both in the tracker
    /// and the [`Indexer`], are first removed from the [`Indexer`] and on success get their reference count
    /// decreased.
    ///
    /// On success, returns the addresses that where successfully removed from the [`Indexer`].
    ///
    /// Fails, leaving the tracker unchanged, if:
    ///
    /// - the tracker does not have `Some` prefix,
    /// - some address did not match the tracker prefix.
    fn unregister<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut addresses: Vec<Address>) -> Result<Vec<Address>> {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker().inner));

        let mut counter: usize = 0;
        let mut inner = self.inner.write();
        if let Some(prefix) = inner.prefix {
            // Make sure all addresses match the tracker prefix
            if let Some(address) = addresses.iter().find(|address| address.prefix != prefix) {
                return Err(Error::PrefixMismatch(address.clone()));
            }

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
        } else {
            return Err(Error::NoPrefix);
        }
        Ok(addresses)
    }

    /// Unregisters the indexes contained in an [`Indexes`] from an [`Indexer`].
    ///
    /// Returns the indexes that were successfully removed from the [`Indexer`].
    fn unregister_indexes<T: Indexer + IndexerStorage>(&self, indexes: &mut T, mut other: Indexes) -> Indexes {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker().inner));
        assert!(Arc::ptr_eq(&self.inner, &other.tracker.inner));

        let mut counter: usize = 0;
        let mut inner = self.inner.write();
        other.indexes.retain(|index| {
            counter += 1;
            if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                RwLockWriteGuard::bump(&mut inner);
            }
            // Decrease `index` ref count since one of the following cases occurs:
            // a) `indexes` removes the index (-1) and `other` retains it (0)
            // b) `indexes` keeps the index (0) and `other` does not retain it (-1)
            inner.dec_count(*index);

            indexes.remove(*index)
        });
        other
    }

    fn reference_indexes(&self, indexes: impl Iterator<Item = Index>) {
        for chunk in &indexes.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|index| inner.inc_count(index));
        }
    }

    /// Unregisters all indexes contained in `indexes`, draining it in the process.
    fn clear_indexes(&self, indexes: &mut Indexes) {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker.inner));

        for chunk in &indexes.indexes.drain().chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            // Any index existing in `indexes` was registered in the tracker so it must get
            // its tracked reference count decreased.
            chunk.for_each(|index| inner.dec_count(index));
        }
    }

    /// Unregisters all indexes contained in `counters`, draining it in the process.
    fn clear_counters(&self, counters: &mut Counters) {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &counters.tracker.inner));

        for chunk in &counters.indexes.drain().chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|(index, counter)| {
                // If the counter is non-zero, its associated index is currently tracked here so
                // it must get its tracked reference count decreased.
                if counter > 0 {
                    inner.dec_count(index)
                }
            });
        }
        counters.empty_entries = 0;
    }

    /// Converts the indexes of an [`Indexer`] into a vector of addresses.
    pub fn to_addresses<T: Indexer>(&self, indexes: &T) -> Vec<Address> {
        // Make sure all objects are bound to this tracker
        assert!(Arc::ptr_eq(&self.inner, &indexes.tracker().inner));

        let mut addresses = Vec::with_capacity(indexes.len());
        let mut counter: usize = 0;
        let mut inner = self.inner.write();
        indexes.iter_index().for_each(|index| {
            counter += 1;
            if counter % Self::ADDRESS_CHUNK_SIZE == 0 {
                RwLockWriteGuard::bump(&mut inner);
            }
            if let Some(address) = inner.get_index_address(index) {
                addresses.push(address);
            }
        });
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
    use crate::address::test_helpers::ADDRESS_PREFIX;
    use kaspa_math::Uint256;

    fn create_addresses(start: usize, count: usize) -> Vec<Address> {
        (start..start + count)
            .map(|i| Address::new(ADDRESS_PREFIX, kaspa_addresses::Version::PubKey, &Uint256::from_u64(i as u64).to_le_bytes()))
            .collect()
    }

    #[test]
    fn test_tracker_capacity_and_entry_recycling() {
        const INIT_MAX_ADDRESSES: usize = 6;
        const MAX_ADDRESSES: usize = ((INIT_MAX_ADDRESSES + 1) * 8 / 7).next_power_of_two() * 7 / 8 - 1;
        const CAPACITY: usize = MAX_ADDRESSES + 1;

        let tracker = Tracker::new(Some(ADDRESS_PREFIX), Some(MAX_ADDRESSES));
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
        let mut idx_a = Indexes::new(tracker.clone());
        let aa = idx_a.register(aa).unwrap();
        let aai = aa.iter().map(|x| tracker.get_address(x).unwrap().0).collect_vec();
        assert_eq!(aa.len(), MAX_ADDRESSES, "all addresses should be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "all addresses should be registered");
        for i in 0..aa.len() {
            assert!(idx_a.contains(&aa[i]), "tracker should contain the registered address");
            assert!(idx_a.contains_index(aai[i]), "index set should contain the registered address index");
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
        let mut idx_b = Indexes::new(tracker.clone());
        let ab = idx_b.register(create_addresses(1, AB_COUNT)).unwrap();
        assert_eq!(ab.len(), AB_COUNT, "all addresses should be registered");
        assert_eq!(idx_b.len(), AB_COUNT, "all addresses should be registered");

        // Empty the tracker entry containing A0
        assert_eq!(idx_a.unregister(create_addresses(0, 1)).unwrap().len(), 1);
        assert_eq!(idx_a.len(), MAX_ADDRESSES - 1, "entry #0 with address A0 should now be marked empty");

        // Fill the empty entry with a single new address A8
        const AC_COUNT: usize = 1;
        let ac = idx_a.register(create_addresses(MAX_ADDRESSES, AC_COUNT)).unwrap();
        let aci = ac.iter().map(|x| tracker.get_address(x).unwrap().0).collect_vec();
        assert_eq!(ac.len(), AC_COUNT, "a new address should be registered");
        assert_eq!(idx_a.len(), MAX_ADDRESSES, "a new address should be registered");
        assert_eq!(ac[0], create_addresses(MAX_ADDRESSES, AC_COUNT)[0], "the new address A8 should be registered");
        assert!(idx_a.contains(&ac[0]), "the new address A8 should be registered");
        assert_eq!(aai[0], aci[0], "the newly registered address A8 should occupy the previously emptied entry");

        assert_eq!(
            tracker.capacity(),
            CAPACITY,
            "the tracker capacity should not have been affected by the transient insert/swap_remove"
        );
    }

    #[test]
    fn test_indexes_eq() {
        let tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let i1 = IndexSet::test_with_indexes(tracker.clone(), vec![0, 1, 2, 3, 5, 7, 11]);
        let i2 = IndexSet::test_with_indexes(tracker.clone(), vec![5, 7, 11, 0, 1, 2, 3]);
        let i3 = IndexSet::test_with_indexes(tracker.clone(), vec![0, 1, 2, 4, 8, 16, 32]);
        let i4 = IndexSet::test_with_indexes(tracker.clone(), vec![0, 1]);
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

    struct Test {
        tracker: Tracker,
        addresses: Vec<Address>,
    }

    impl Test {
        fn assert_ci(
            &self,
            label: &str,
            c: &CounterMap,
            c_counters: &[RefCount],
            i: &IndexSet,
            i_bits: &[RefCount],
            tracker_counters: &[RefCount],
        ) {
            self.assert_tracker(label, tracker_counters);
            self.assert_counter_map(label, c, c_counters);
            self.assert_index_set(label, i, i_bits);
        }

        fn assert_c(&self, label: &str, item: &CounterMap, item_counters: &[RefCount], tracker_counters: &[RefCount]) {
            self.assert_tracker(label, tracker_counters);
            self.assert_counter_map(label, item, item_counters);
        }

        fn assert_ii(
            &self,
            label: &str,
            c: &IndexSet,
            c_bits: &[RefCount],
            i: &IndexSet,
            i_bits: &[RefCount],
            tracker_counters: &[RefCount],
        ) {
            self.assert_tracker(label, tracker_counters);
            self.assert_index_set(label, c, c_bits);
            self.assert_index_set(label, i, i_bits);
        }

        fn assert_i(&self, label: &str, item: &IndexSet, item_bits: &[RefCount], tracker_counters: &[RefCount]) {
            self.assert_tracker(label, tracker_counters);
            self.assert_index_set(label, item, item_bits);
        }

        fn assert_tracker(&self, label: &str, counters: &[RefCount]) {
            assert_eq!(self.tracker.len(), counters.iter().filter(|x| **x > 0).count(), "{}: length should match", label);
            let tracker_counters = (0..counters.len()).map(|i| self.tracker.get_address(&self.addresses[i]).unwrap().1).collect_vec();
            assert_eq!(tracker_counters, *counters, "{}: counters should match", label);
        }

        fn assert_counter_map(&self, label: &str, item: &CounterMap, counters: &[RefCount]) {
            assert_eq!(item.len(), counters.iter().filter(|x| **x > 0).count(), "{}: length should match", label);
            let item_counters =
                (0..counters.len()).map(|i| item.test_get_address(&self.addresses[i]).unwrap_or_default()).collect_vec();
            assert_eq!(item_counters, *counters, "{}: counters should match", label);
        }

        fn assert_index_set(&self, label: &str, item: &IndexSet, bits: &[RefCount]) {
            assert_eq!(item.len(), bits.iter().filter(|x| **x > 0).count(), "{}: length should match", label);
            let item_counters = (0..bits.len()).map(|i| if item.test_get_address(&self.addresses[i]) { 1 } else { 0 }).collect_vec();
            assert_eq!(item_counters, *bits, "{}: bits should match", label);
        }
    }

    #[test]
    fn test_counter_map_tracking() {
        let tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let addresses = create_addresses(0, 3);
        let test = Test { tracker: tracker.clone(), addresses };

        let mut c1 = CounterMap::new(tracker.clone());
        test.assert_c("c1: new", &c1, &[], &[]);
        assert_eq!(c1.register(test.addresses.clone()).unwrap(), test.addresses);
        test.assert_c("c1: register [0, 1 ,2]", &c1, &[1, 1, 1], &[1, 1, 1]);
        assert_eq!(c1.register(test.addresses[0..=1].to_vec()).unwrap(), vec![]);
        test.assert_c("c1: register [0, 1]", &c1, &[2, 2, 1], &[1, 1, 1]);

        let mut c2 = c1.clone();
        test.assert_c("c2: clone c1", &c2, &[2, 2, 1], &[2, 2, 2]);

        let mut c3 = c1.clone();
        test.assert_c("c3: clone c1", &c3, &[2, 2, 1], &[3, 3, 3]);
        c3.clear();
        test.assert_c("c3: clear", &c3, &[0, 0, 0], &[2, 2, 2]);
        drop(c3);
        test.assert_tracker("c3: drop", &[2, 2, 2]);

        assert_eq!(c2.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[2..=2].to_vec());
        test.assert_c("c2: unregister [1, 2]", &c2, &[2, 1, 0], &[2, 2, 1]);
        assert_eq!(c2.unregister(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[1..=1].to_vec());
        test.assert_c("c2: unregister [0, 1]", &c2, &[1, 0, 0], &[2, 1, 1]);
        assert_eq!(c2.unregister(test.addresses[0..=0].to_vec()).unwrap(), test.addresses[0..=0].to_vec());
        test.assert_c("c2: unregister [0]", &c2, &[], &[1, 1, 1]);
        drop(c2);
        test.assert_tracker("c2: drop", &[1, 1, 1]);

        assert_eq!(c1.unregister(test.addresses[0..=0].to_vec()).unwrap(), vec![]);
        test.assert_c("c1: unregister [0]", &c1, &[1, 2, 1], &[1, 1, 1]);
        assert_eq!(c1.unregister(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[0..=0].to_vec());
        test.assert_c("c1: unregister [0, 1]", &c1, &[0, 1, 1], &[0, 1, 1]);

        let mut c4 = c1.clone();
        test.assert_c("c4: clone c1", &c4, &[0, 1, 1], &[0, 2, 2]);

        assert_eq!(c1.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[1..=2].to_vec());
        test.assert_c("c1: unregister [1, 2]", &c1, &[0, 0, 0], &[0, 1, 1]);

        drop(c1);
        test.assert_tracker("c1: drop", &[0, 1, 1]);

        assert_eq!(c4.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[1..=2].to_vec());
        test.assert_c("c4: unregister [1, 2]", &c4, &[0, 0, 0], &[0, 0, 0]);

        drop(c4);
        test.assert_tracker("c4: drop", &[0, 0, 0]);
    }

    #[test]
    fn test_counter_map_tracking_indexes() {
        let tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let addresses = create_addresses(0, 3);
        let test = Test { tracker: tracker.clone(), addresses };

        let mut c1 = CounterMap::new(tracker.clone());
        test.assert_c("c1: new", &c1, &[], &[]);
        assert_eq!(c1.register(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[0..=1].to_vec());
        test.assert_c("c1: register [0, 1]", &c1, &[1, 1], &[1, 1]);
        assert_eq!(c1.register(test.addresses[0..=0].to_vec()).unwrap(), vec![]);
        test.assert_c("c1: register [0]", &c1, &[2, 1], &[1, 1]);

        let mut i1 = IndexSet::new(tracker.clone());
        assert_eq!(i1.register(test.addresses.clone()).unwrap(), test.addresses);
        test.assert_i("i1: register [0, 1 ,2]", &i1, &[1, 1, 1], &[2, 2, 1]);

        let i2 = c1.register_indexes(i1);
        test.assert_ci("i2: c1.register_indexes(i1)", &c1, &[3, 2, 1], &i2, &[0, 0, 1], &[1, 1, 2]);

        let mut i3 = IndexSet::new(tracker.clone());
        assert_eq!(i3.register(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[1..=2].to_vec());
        test.assert_i("i3: register [1 ,2]", &i3, &[0, 1, 1], &[1, 2, 3]);

        let i4 = c1.unregister_indexes(i3);
        test.assert_ci("i4: c1.unregister_indexes(i3)", &c1, &[3, 1, 0], &i4, &[0, 0, 1], &[1, 1, 2]);
        drop(i2);
        drop(i4);
        test.assert_tracker("i2, i4: drop", &[1, 1, 0]);

        let i5 = c1.get_indexes();
        test.assert_ci("i5: c1.get_indexes()", &c1, &[3, 1, 0], &i5, &[1, 1, 0], &[2, 2, 0]);
        let i6 = c1.register_indexes(i5);
        test.assert_ci("i6: c1.register_indexes(i5)", &c1, &[4, 2, 0], &i6, &[0, 0, 0], &[1, 1, 0]);

        let _ = c1.unregister_indexes(c1.get_indexes());
        let i7 = c1.unregister_indexes(c1.get_indexes());
        test.assert_ci("i7: 2 x c1.unregister_indexes(get_index)", &c1, &[2, 0, 0], &i7, &[0, 1, 0], &[1, 1, 0]);
        drop(i7);
        test.assert_tracker("i7: drop", &[1, 0, 0]);

        let _ = c1.unregister_indexes(c1.get_indexes());
        let i8 = c1.unregister_indexes(c1.get_indexes());
        test.assert_ci("i8: 2 x c1.unregister_indexes(get_index)", &c1, &[0, 0, 0], &i8, &[1, 0, 0], &[1, 0, 0]);
        drop(i8);
        test.assert_tracker("i8: drop", &[0, 0, 0]);

        drop(c1);
        test.assert_tracker("c1: drop", &[0, 0, 0]);
    }

    #[test]
    fn test_index_set_tracking() {
        let tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let addresses = create_addresses(0, 3);
        let test = Test { tracker: tracker.clone(), addresses };

        let mut c1 = IndexSet::new(tracker.clone());
        test.assert_i("c1: new", &c1, &[], &[]);
        assert_eq!(c1.register(test.addresses.clone()).unwrap(), test.addresses);
        test.assert_i("c1: register [0, 1 ,2]", &c1, &[1, 1, 1], &[1, 1, 1]);
        assert_eq!(c1.register(test.addresses[0..=1].to_vec()).unwrap(), vec![]);
        test.assert_i("c1: register [0, 1]", &c1, &[1, 1, 1], &[1, 1, 1]);

        let mut c2 = c1.clone();
        test.assert_i("c2: clone c1", &c2, &[1, 1, 1], &[2, 2, 2]);

        let mut c3 = c1.clone();
        test.assert_i("c3: clone c1", &c3, &[1, 1, 1], &[3, 3, 3]);
        c3.clear();
        test.assert_i("c3: clear", &c3, &[0, 0, 0], &[2, 2, 2]);
        drop(c3);
        test.assert_tracker("c3: drop", &[2, 2, 2]);

        assert_eq!(c2.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[1..=2].to_vec());
        test.assert_i("c2: unregister [1, 2]", &c2, &[1, 0, 0], &[2, 1, 1]);
        assert_eq!(c2.unregister(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[0..=0].to_vec());
        test.assert_i("c2: unregister [0, 1]", &c2, &[0, 0, 0], &[1, 1, 1]);
        assert_eq!(c2.unregister(test.addresses[0..=0].to_vec()).unwrap(), vec![]);
        test.assert_i("c2: unregister [0]", &c2, &[], &[1, 1, 1]);
        drop(c2);
        test.assert_tracker("c2: drop", &[1, 1, 1]);

        assert_eq!(c1.unregister(test.addresses[0..=0].to_vec()).unwrap(), test.addresses[0..=0].to_vec());
        test.assert_i("c1: unregister [0]", &c1, &[0, 1, 1], &[0, 1, 1]);
        assert_eq!(c1.unregister(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[1..=1].to_vec());
        test.assert_i("c1: unregister [0, 1]", &c1, &[0, 0, 1], &[0, 0, 1]);

        let mut c4 = c1.clone();
        test.assert_i("c4: clone c1", &c4, &[0, 0, 1], &[0, 0, 2]);

        assert_eq!(c1.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[2..=2].to_vec());
        test.assert_i("c1: unregister [1, 2]", &c1, &[0, 0, 0], &[0, 0, 1]);

        drop(c1);
        test.assert_tracker("c1: drop", &[0, 0, 1]);

        assert_eq!(c4.unregister(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[2..=2].to_vec());
        test.assert_i("c4: unregister [1, 2]", &c4, &[0, 0, 0], &[0, 0, 0]);

        drop(c4);
        test.assert_tracker("c4: drop", &[0, 0, 0]);
    }

    #[test]
    fn test_index_set_tracking_indexes() {
        let tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let addresses = create_addresses(0, 3);
        let test = Test { tracker: tracker.clone(), addresses };

        let mut c1 = IndexSet::new(tracker.clone());
        test.assert_i("c1: new", &c1, &[], &[]);
        assert_eq!(c1.register(test.addresses[0..=1].to_vec()).unwrap(), test.addresses[0..=1].to_vec());
        test.assert_i("c1: register [0, 1]", &c1, &[1, 1], &[1, 1]);
        assert_eq!(c1.register(test.addresses[0..=0].to_vec()).unwrap(), vec![]);
        test.assert_i("c1: register [0]", &c1, &[1, 1], &[1, 1]);

        let mut i1 = IndexSet::new(tracker.clone());
        assert_eq!(i1.register(test.addresses.clone()).unwrap(), test.addresses);
        test.assert_i("i1: register [0, 1 ,2]", &i1, &[1, 1, 1], &[2, 2, 1]);

        let i2 = c1.register_indexes(i1);
        test.assert_ii("i2: c1.register_indexes(i1)", &c1, &[1, 1, 1], &i2, &[0, 0, 1], &[1, 1, 2]);

        let mut i3 = IndexSet::new(tracker.clone());
        assert_eq!(i3.register(test.addresses[1..=2].to_vec()).unwrap(), test.addresses[1..=2].to_vec());
        test.assert_i("i3: register [1 ,2]", &i3, &[0, 1, 1], &[1, 2, 3]);

        let mut i4 = c1.unregister_indexes(i3);
        test.assert_ii("i4: c1.unregister_indexes(i3)", &c1, &[1, 0, 0], &i4, &[0, 1, 1], &[1, 1, 2]);

        drop(i2);
        test.assert_tracker("i2: drop", &[1, 1, 1]);

        let i5 = i4.transfer();
        test.assert_ii("i5: i4.transfer()", &i4, &[0, 0, 0], &i5, &[0, 1, 1], &[1, 1, 1]);
        drop(i4);
        test.assert_tracker("i4: drop", &[1, 1, 1]);

        let i6 = c1.unregister_indexes(i5);
        test.assert_ii("c1: i4.unregister_indexes(i5)", &c1, &[1, 0, 0], &i6, &[0, 0, 0], &[1, 0, 0]);
        drop(i6);
        test.assert_tracker("i6: drop", &[1, 0, 0]);

        drop(c1);
        test.assert_tracker("c1: drop", &[0, 0, 0]);
    }
}
