use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::Rng;
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[derive(Clone)]
pub struct Cache<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S = RandomState> {
    // We use IndexMap and not HashMap, because it makes it cheaper to remove a random element when the cache is full.
    map: Arc<RwLock<IndexMap<TKey, TData, S>>>,
    size: usize,
    counters: Arc<TxScriptCacheCounters>,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S: BuildHasher + Default> Cache<TKey, TData, S> {
    pub fn new(size: u64) -> Self {
        Self::with_counters(size, Default::default())
    }

    pub fn with_counters(size: u64, counters: Arc<TxScriptCacheCounters>) -> Self {
        Self {
            map: Arc::new(RwLock::new(IndexMap::with_capacity_and_hasher(size as usize, S::default()))),
            size: size as usize,
            counters,
        }
    }

    pub fn clear(&self) {
        self.map.write().clear();
    }

    pub(crate) fn get(&self, key: &TKey) -> Option<TData> {
        self.map.read().get(key).cloned().inspect(|_data| {
            self.counters.get_counts.fetch_add(1, Ordering::Relaxed);
        })
    }

    pub(crate) fn insert(&self, key: TKey, data: TData) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        if write_guard.len() == self.size {
            write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
        }
        write_guard.insert(key, data);
        self.counters.insert_counts.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct TxScriptCacheCounters {
    pub insert_counts: AtomicU64,
    pub get_counts: AtomicU64,
}

impl TxScriptCacheCounters {
    pub fn snapshot(&self) -> TxScriptCacheCountersSnapshot {
        TxScriptCacheCountersSnapshot {
            insert_counts: self.insert_counts.load(Ordering::Relaxed),
            get_counts: self.get_counts.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TxScriptCacheCountersSnapshot {
    pub insert_counts: u64,
    pub get_counts: u64,
}

impl TxScriptCacheCountersSnapshot {
    pub fn hit_ratio(&self) -> f64 {
        if self.insert_counts > 0 {
            self.get_counts as f64 / self.insert_counts as f64
        } else {
            0f64
        }
    }
}

impl core::ops::Sub for &TxScriptCacheCountersSnapshot {
    type Output = TxScriptCacheCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            insert_counts: self.insert_counts.saturating_sub(rhs.insert_counts),
            get_counts: self.get_counts.saturating_sub(rhs.get_counts),
        }
    }
}
