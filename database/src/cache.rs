use indexmap::IndexMap;
use kaspa_utils::mem_size::MemSizeEstimator;
use parking_lot::RwLock;
use rand::Rng;
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    /// An empty cache (avoids aqcuiring locks etc so considred perf-free)
    Empty,
    /// The cache bounds the number of items it holds w/o tracking their inner size
    Unit(usize),
    /// Items are tracked by size and the cache never allows the accumulated tracked size
    /// to surpass the provided size argument.
    Tracked(usize, bool),
    /// Items are tracked by size with a max_size limit but the cache will pass this limit if
    /// there are no more than min_units overall in the cache
    LowerBoundedTracked { max_size: usize, min_units: usize, bytes_mode: bool },
}

#[derive(Clone)]
struct CachePolicyInner {
    /// Indicates if this cache was set to be tracked.
    tracked: bool,
    /// The max size of this cache. Units (bytes or some logical unit) depend on
    /// caller logic and the implementation of `MemSizeEstimator`.
    max_size: usize,
    /// Min units to keep in the cache even if passing tracked size limit.
    min_units: usize,
    /// Indicates whether tracking is in bytes mode
    bytes_mode: bool,
}

impl From<CachePolicy> for CachePolicyInner {
    fn from(policy: CachePolicy) -> Self {
        match policy {
            CachePolicy::Empty => CachePolicyInner { tracked: false, max_size: 0, min_units: 0, bytes_mode: false },
            CachePolicy::Unit(max_size) => CachePolicyInner { tracked: false, max_size, min_units: 0, bytes_mode: false },
            CachePolicy::Tracked(max_size, bytes_mode) => CachePolicyInner { tracked: true, max_size, min_units: 0, bytes_mode },
            CachePolicy::LowerBoundedTracked { max_size, min_units, bytes_mode } => {
                CachePolicyInner { tracked: true, max_size, min_units, bytes_mode }
            }
        }
    }
}

struct Inner<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
{
    // We use IndexMap and not HashMap because it makes it cheaper to remove a random element when the cache is full.
    map: IndexMap<TKey, TData, S>,
    tracked_size: usize,
}

impl<TKey, TData, S> Inner<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    /// Evicts items until meeting cache policy requirements
    fn evict(&mut self, policy: &CachePolicyInner) {
        // We allow passing tracked size limit as long as there are no more than min_units units
        while self.tracked_size > policy.max_size && self.map.len() > policy.min_units {
            if let Some((_, v)) = self.map.swap_remove_index(rand::thread_rng().gen_range(0..self.map.len())) {
                self.tracked_size -= v.estimate_size(policy.bytes_mode)
            }
        }
    }

    fn insert(&mut self, policy: &CachePolicyInner, key: TKey, data: TData) {
        if policy.tracked {
            let new_data_size = data.estimate_size(policy.bytes_mode);
            self.tracked_size += new_data_size;
            if let Some(removed) = self.map.insert(key, data) {
                self.tracked_size -= removed.estimate_size(policy.bytes_mode);
            }
            self.evict(policy);
        } else {
            if self.map.len() == policy.max_size {
                self.map.swap_remove_index(rand::thread_rng().gen_range(0..policy.max_size));
            }
            self.map.insert(key, data);
        }
    }

    fn update_if_entry_exists<F>(&mut self, policy: &CachePolicyInner, key: TKey, op: F)
    where
        F: Fn(&mut TData),
    {
        if let Some(data) = self.map.get_mut(&key) {
            if policy.tracked {
                self.tracked_size -= data.estimate_size(policy.bytes_mode);
                op(data);
                self.tracked_size += data.estimate_size(policy.bytes_mode);
                self.evict(policy);
            } else {
                op(data);
            }
        }
    }

    fn remove(&mut self, policy: &CachePolicyInner, key: &TKey) -> Option<TData> {
        match self.map.swap_remove(key) {
            Some(data) => {
                if policy.tracked {
                    self.tracked_size -= data.estimate_size(policy.bytes_mode);
                }
                Some(data)
            }
            None => None,
        }
    }
}

impl<TKey, TData, S> Inner<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    pub fn new(prealloc_size: usize) -> Self {
        Self { map: IndexMap::with_capacity_and_hasher(prealloc_size, S::default()), tracked_size: 0 }
    }
}

#[derive(Clone)]
pub struct Cache<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
{
    inner: Arc<RwLock<Inner<TKey, TData, S>>>,
    policy: CachePolicyInner,
}

impl<TKey, TData, S> Cache<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    pub fn new(policy: CachePolicy) -> Self {
        let policy: CachePolicyInner = policy.into();
        let prealloc_size = if policy.tracked { 0 } else { policy.max_size };
        Self { inner: Arc::new(RwLock::new(Inner::new(prealloc_size))), policy }
    }

    pub fn get(&self, key: &TKey) -> Option<TData> {
        self.inner.read().map.get(key).cloned()
    }

    pub fn contains_key(&self, key: &TKey) -> bool {
        self.inner.read().map.contains_key(key)
    }

    pub fn insert(&self, key: TKey, data: TData) {
        if self.policy.max_size == 0 {
            return;
        }

        self.inner.write().insert(&self.policy, key, data);
    }

    pub fn insert_many(&self, iter: &mut impl Iterator<Item = (TKey, TData)>) {
        if self.policy.max_size == 0 {
            return;
        }
        let mut inner = self.inner.write();
        for (key, data) in iter {
            inner.insert(&self.policy, key, data);
        }
    }

    pub fn update_if_entry_exists<F>(&self, key: TKey, op: F)
    where
        F: Fn(&mut TData),
    {
        if self.policy.max_size == 0 {
            return;
        }
        self.inner.write().update_if_entry_exists(&self.policy, key, op);
    }

    pub fn remove(&self, key: &TKey) -> Option<TData> {
        if self.policy.max_size == 0 {
            return None;
        }
        self.inner.write().remove(&self.policy, key)
    }

    pub fn remove_many(&self, key_iter: &mut impl Iterator<Item = TKey>) {
        if self.policy.max_size == 0 {
            return;
        }
        let mut inner = self.inner.write();
        for key in key_iter {
            inner.remove(&self.policy, &key);
        }
    }

    pub fn remove_all(&self) {
        if self.policy.max_size == 0 {
            return;
        }
        let mut inner = self.inner.write();
        inner.map.clear();
        if self.policy.tracked {
            inner.tracked_size = 0;
        }
    }
}
