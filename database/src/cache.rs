use indexmap::IndexMap;
use kaspa_utils::mem_size::MemSizeEstimator;
use parking_lot::RwLock;
use rand::Rng;
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    Unit,
    Tracked,
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
    pub fn new(max_size: u64) -> Self {
        // Use `size + 1` for not triggering a realloc if new element exactly overflows capacity
        Self { map: IndexMap::with_capacity_and_hasher(max_size as usize + 1, S::default()), tracked_size: 0 }
    }
}

#[derive(Clone)]
pub struct Cache<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
{
    inner: Arc<RwLock<Inner<TKey, TData, S>>>,
    max_size: usize,
    policy: CachePolicy,
}

impl<TKey, TData, S> Cache<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    pub fn new(size: u64) -> Self {
        // TODO: policy and prealloc strategy
        Self { inner: Arc::new(RwLock::new(Inner::new(size))), max_size: size as usize, policy: CachePolicy::Unit }
    }

    pub fn get(&self, key: &TKey) -> Option<TData> {
        self.inner.read().map.get(key).cloned()
    }

    pub fn contains_key(&self, key: &TKey) -> bool {
        self.inner.read().map.contains_key(key)
    }

    fn insert_impl(&self, inner: &mut Inner<TKey, TData, S>, key: TKey, data: TData) {
        match self.policy {
            CachePolicy::Unit => {
                if inner.map.len() == self.max_size {
                    inner.map.swap_remove_index(rand::thread_rng().gen_range(0..self.max_size));
                }
                inner.map.insert(key, data);
            }
            CachePolicy::Tracked => {
                let new_data_size = data.estimate_mem_size().agnostic_size();
                inner.tracked_size += new_data_size;
                if let Some(removed) = inner.map.insert(key, data) {
                    // TODO: underflow
                    inner.tracked_size -= removed.estimate_mem_size().agnostic_size();
                }

                while inner.tracked_size > self.max_size {
                    if let Some((_, v)) = inner.map.swap_remove_index(rand::thread_rng().gen_range(0..inner.map.len())) {
                        // TODO: underflow
                        inner.tracked_size -= v.estimate_mem_size().agnostic_size();
                    }
                }
            }
        }
    }

    pub fn insert(&self, key: TKey, data: TData) {
        if self.max_size == 0 {
            return;
        }

        let mut write_guard = self.inner.write();
        self.insert_impl(&mut write_guard, key, data);
    }

    pub fn insert_many(&self, iter: &mut impl Iterator<Item = (TKey, TData)>) {
        if self.max_size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        for (key, data) in iter {
            self.insert_impl(&mut write_guard, key, data);
        }
    }

    fn remove_impl(&self, inner: &mut Inner<TKey, TData, S>, key: &TKey) -> Option<TData> {
        match inner.map.swap_remove(key) {
            Some(data) => {
                // TODO: underflow
                if self.policy == CachePolicy::Tracked {
                    inner.tracked_size -= data.estimate_mem_size().agnostic_size();
                }
                Some(data)
            }
            None => None,
        }
    }

    pub fn remove(&self, key: &TKey) -> Option<TData> {
        if self.max_size == 0 {
            return None;
        }
        let mut write_guard = self.inner.write();
        self.remove_impl(&mut write_guard, key)
    }

    pub fn remove_many(&self, key_iter: &mut impl Iterator<Item = TKey>) {
        if self.max_size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        for key in key_iter {
            self.remove_impl(&mut write_guard, &key);
        }
    }

    pub fn remove_all(&self) {
        if self.max_size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        write_guard.map.clear();
        if self.policy == CachePolicy::Tracked {
            write_guard.tracked_size = 0;
        }
    }
}
