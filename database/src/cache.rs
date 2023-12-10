use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::Rng;
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::Arc};

struct Inner<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S = RandomState> {
    // We use IndexMap and not HashMap, because it makes it cheaper to remove a random element when the cache is full.
    map: IndexMap<TKey, TData, S>,
    _tracked_size: usize,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S: BuildHasher + Default> Inner<TKey, TData, S> {
    pub fn new(size: u64) -> Self {
        Self { map: IndexMap::with_capacity_and_hasher(size as usize, S::default()), _tracked_size: 0 }
    }
}

#[derive(Clone)]
pub struct Cache<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S = RandomState> {
    inner: Arc<RwLock<Inner<TKey, TData, S>>>,
    size: usize,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S: BuildHasher + Default> Cache<TKey, TData, S> {
    pub fn new(size: u64) -> Self {
        Self { inner: Arc::new(RwLock::new(Inner::new(size))), size: size as usize }
    }

    pub fn get(&self, key: &TKey) -> Option<TData> {
        self.inner.read().map.get(key).cloned()
    }

    pub fn contains_key(&self, key: &TKey) -> bool {
        self.inner.read().map.contains_key(key)
    }

    pub fn insert(&self, key: TKey, data: TData) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        if write_guard.map.len() == self.size {
            write_guard.map.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
        }
        write_guard.map.insert(key, data);
    }

    pub fn insert_many(&self, iter: &mut impl Iterator<Item = (TKey, TData)>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        for (key, data) in iter {
            if write_guard.map.len() == self.size {
                write_guard.map.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
            }
            write_guard.map.insert(key, data);
        }
    }

    pub fn remove(&self, key: &TKey) -> Option<TData> {
        if self.size == 0 {
            return None;
        }
        let mut write_guard = self.inner.write();
        write_guard.map.swap_remove(key)
    }

    pub fn remove_many(&self, key_iter: &mut impl Iterator<Item = TKey>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        for key in key_iter {
            write_guard.map.swap_remove(&key);
        }
    }

    pub fn remove_all(&self) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.inner.write();
        write_guard.map.clear()
    }
}
