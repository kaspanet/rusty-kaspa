use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::trace;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use parking_lot::RwLock;
use std::{
    collections::{hash_map, HashMap, HashSet},
    fmt::Display,
};

pub trait Indexer {
    fn contains(&self, index: Index) -> bool;
    fn insert(&mut self, index: Index) -> bool;
    fn remove(&mut self, index: Index) -> bool;
    fn unlock(&mut self);
}

pub type Index = u32;
pub type RefCount = u16;

/// Tracks reference count of indexes
///
/// Can be assigned to `CounterMap` or `CounterVec` that provide alternate implementations.
pub type Counters = CounterMap;

/// Tracks indexes
///
/// Can be assigned to `IndexSet` or `IndexVec` that provide alternate implementations.
pub type Indexes = IndexSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterMap(HashMap<Index, RefCount>);

impl CounterMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn with_counters(counters: Vec<Counter>) -> Self {
        Self(counters.into_iter().map(|x| (x.index, x.count)).collect())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> hash_map::Iter<'_, Index, RefCount> {
        self.0.iter()
    }

    pub fn chunks(&self, chunk_size: usize) -> itertools::IntoChunks<hash_map::Iter<'_, Index, RefCount>> {
        self.0.iter().chunks(chunk_size)
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

    fn unlock(&mut self) {}
}

#[derive(Debug, Clone)]
pub struct Counter {
    pub index: Index,
    pub count: RefCount,
    pub locked: bool,
}

impl Counter {
    pub fn new(index: Index, count: RefCount) -> Self {
        Self { index, count, locked: false }
    }

    pub fn active(&self) -> bool {
        self.count > 0
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn unlock(&mut self) {
        self.locked = false
    }
}

impl PartialEq for Counter {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl Eq for Counter {}

impl PartialOrd for Counter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Counter {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterVec(Vec<Counter>);

impl CounterVec {
    pub fn new(mut counters: Vec<Counter>) -> Self {
        counters.sort();
        Self(counters)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Index, &RefCount)> {
        self.0.iter().map(|x| (&x.index, &x.count))
    }

    pub fn chunks(&self, chunk_size: usize) -> itertools::IntoChunks<impl Iterator<Item = (&Index, &RefCount)>> {
        self.iter().chunks(chunk_size)
    }
}

impl Indexer for CounterVec {
    fn contains(&self, index: Index) -> bool {
        self.0.binary_search(&Counter::new(index, 0)).is_ok()
    }

    fn insert(&mut self, index: Index) -> bool {
        let item = Counter { index, count: 1, locked: true };
        match self.0.binary_search(&item) {
            Ok(rank) => {
                let counter = self.0.get_mut(rank).unwrap();
                if !counter.locked {
                    counter.locked = true;
                    counter.count += 1;
                }
                counter.count == 1
            }
            Err(rank) => {
                self.0.insert(rank, item);
                true
            }
        }
    }

    fn remove(&mut self, index: Index) -> bool {
        match self.0.binary_search(&Counter::new(index, 0)) {
            Ok(rank) => {
                let counter = self.0.get_mut(rank).unwrap();
                if counter.count > 0 && !counter.locked {
                    counter.locked = true;
                    counter.count -= 1;
                    counter.count == 0
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    fn unlock(&mut self) {
        self.0.iter_mut().for_each(Counter::unlock);
    }
}

#[derive(Debug, Clone)]
pub struct IndexSet(HashSet<Index>);

impl IndexSet {
    pub fn new(indexes: Vec<Index>) -> Self {
        Self(indexes.into_iter().collect())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Index> {
        self.0.iter()
    }

    pub fn chunks(&self, chunk_size: usize) -> itertools::IntoChunks<impl Iterator<Item = &Index>> {
        self.0.iter().chunks(chunk_size)
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

    fn unlock(&mut self) {}
}

impl From<Vec<Index>> for IndexSet {
    fn from(item: Vec<Index>) -> Self {
        Self::new(item)
    }
}

#[derive(Debug, Clone)]
pub struct IndexVec(Vec<Index>);

impl IndexVec {
    pub fn new(mut indexes: Vec<Index>) -> Self {
        indexes.sort();
        Self(indexes)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Index> {
        self.0.iter()
    }

    pub fn chunks(&self, chunk_size: usize) -> itertools::IntoChunks<impl Iterator<Item = &Index>> {
        self.iter().chunks(chunk_size)
    }
}

impl Indexer for IndexVec {
    fn contains(&self, index: Index) -> bool {
        self.0.binary_search(&index).is_ok()
    }

    fn insert(&mut self, index: Index) -> bool {
        match self.0.binary_search(&index) {
            Ok(_) => false,
            Err(rank) => {
                self.0.insert(rank, index);
                true
            }
        }
    }

    fn remove(&mut self, index: Index) -> bool {
        match self.0.binary_search(&index) {
            Ok(rank) => {
                self.0.remove(rank);
                true
            }
            Err(_) => false,
        }
    }

    fn unlock(&mut self) {}
}

impl From<Vec<Index>> for IndexVec {
    fn from(item: Vec<Index>) -> Self {
        Self::new(item)
    }
}

#[derive(Debug)]
struct Inner {
    script_pub_keys: IndexMap<ScriptPublicKey, RefCount>,
}

impl Inner {
    fn new() -> Self {
        Self { script_pub_keys: IndexMap::new() }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self { script_pub_keys: IndexMap::with_capacity(capacity) }
    }

    fn get(&self, spk: &ScriptPublicKey) -> Option<(Index, RefCount)> {
        self.script_pub_keys.get_full(spk).map(|(index, _, count)| (index as Index, *count))
    }

    fn get_index_address(&self, index: Index, prefix: Prefix) -> Option<Address> {
        self.script_pub_keys
            .get_index(index as usize)
            .map(|(spk, _)| extract_script_pub_key_address(spk, prefix).expect("is retro-convertible"))
    }

    fn get_or_insert(&mut self, spk: ScriptPublicKey) -> Index {
        // TODO: reuse entries with counter at 0 when available and some map size threshold is reached
        match self.script_pub_keys.entry(spk) {
            Entry::Occupied(entry) => entry.index() as Index,
            Entry::Vacant(entry) => {
                let index = entry.index() as Index;
                trace!("AddressTracker insert #{} {}", index, extract_script_pub_key_address(entry.key(), Prefix::Mainnet).unwrap());
                let _ = *entry.insert(0);
                index
            }
        }
    }

    fn inc_count(&mut self, index: Index) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(index as usize) {
            *count += 1;
            trace!("AddressTracker inc count #{} to {}", index, *count);
        }
    }

    fn dec_count(&mut self, index: Index) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(index as usize) {
            if *count == 0 {
                panic!("Address tracker is trying to decrease an address counter that is already at zero");
            }
            *count -= 1;
            trace!("AddressTracker dec count #{} to {}", index, *count);
        }
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
    const ADDRESS_CHUNK_SIZE: usize = 256;

    pub fn new() -> Self {
        Self { inner: RwLock::new(Inner::new()) }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { inner: RwLock::new(Inner::with_capacity(capacity)) }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let tracker = Self { inner: RwLock::new(Inner::new()) };
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = tracker.inner.write();
            for address in chunk {
                let _ = inner.get_or_insert(pay_to_address_script(address));
            }
        }
        tracker
    }

    pub fn get(&self, spk: &ScriptPublicKey) -> Option<(Index, RefCount)> {
        self.inner.read().get(spk)
    }

    pub fn get_index_address(&self, index: Index, prefix: Prefix) -> Option<Address> {
        self.inner.read().get_index_address(index, prefix)
    }

    pub fn contains<T: Indexer>(&self, indexes: &T, spk: &ScriptPublicKey) -> bool {
        self.get(spk).is_some_and(|(index, _)| indexes.contains(index))
    }

    pub fn contains_address<T: Indexer>(&self, indexes: &T, address: &Address) -> bool {
        self.contains(indexes, &pay_to_address_script(address))
    }

    pub fn register<T: Indexer>(&self, indexes: &mut T, addresses: &[Address]) -> Vec<Address> {
        let mut added = Vec::with_capacity(addresses.len());
        indexes.unlock();
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            for address in chunk {
                let spk = pay_to_address_script(address);
                let index = inner.get_or_insert(spk);
                if indexes.insert(index) {
                    added.push(address.clone());
                    inner.inc_count(index);
                }
            }
        }
        added
    }

    pub fn unregister<T: Indexer>(&self, indexes: &mut T, addresses: &[Address]) -> Vec<Address> {
        let mut removed = Vec::with_capacity(addresses.len());
        indexes.unlock();
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            for address in chunk {
                let spk = pay_to_address_script(address);
                if let Some((index, _)) = inner.get(&spk) {
                    if indexes.remove(index) {
                        removed.push(address.clone());
                        inner.dec_count(index);
                    }
                }
            }
        }
        removed
    }

    pub fn unregister_indexes(&self, indexes: &Indexes) {
        for chunk in &indexes.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            chunk.for_each(|index| inner.dec_count(*index));
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
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}
