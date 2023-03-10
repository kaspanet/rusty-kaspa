use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::Rng;
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::Arc};

#[derive(Clone)]
pub struct Cache<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S = RandomState> {
    // We use IndexMap and not HashMap, because it makes it cheaper to remove a random element when the cache is full.
    map: Arc<RwLock<IndexMap<TKey, TData, S>>>,
    pub size: usize,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S: BuildHasher + Default> Cache<TKey, TData, S> {
    pub fn new(size: u64) -> Self {
        Self { map: Arc::new(RwLock::new(IndexMap::with_capacity_and_hasher(size as usize, S::default()))), size: size as usize }
    }

    pub fn get(&self, key: &TKey) -> Option<TData> {
        self.map.read().get(key).cloned()
    }

    pub fn contains_key(&self, key: &TKey) -> bool {
        self.map.read().contains_key(key)
    }

    pub fn insert(&self, key: TKey, data: TData) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        if write_guard.len() == self.size {
            write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
        }
        write_guard.insert(key, data);
    }

    pub fn insert_many(&self, iter: &mut impl Iterator<Item = (TKey, TData)>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        for (key, data) in iter {
            if write_guard.len() == self.size {
                write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
            }
            write_guard.insert(key, data);
        }
    }

    pub fn remove(&self, key: &TKey) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        write_guard.swap_remove(key);
    }

    pub fn remove_many(&self, key_iter: &mut impl Iterator<Item = TKey>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        for key in key_iter {
            write_guard.swap_remove(&key);
        }
    }

    pub fn remove_all(&self) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        write_guard.clear()
    }
}
