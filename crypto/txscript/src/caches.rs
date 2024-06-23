use scc::HashCache;
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[derive(Clone)]
pub struct Cache<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S = RandomState>
where
    S: std::hash::BuildHasher,
{
    // We use IndexMap and not HashMap, because it makes it cheaper to remove a random element when the cache is full.
    pub map: Arc<scc::hash_cache::HashCache<TKey, TData, S>>,
    counters: Arc<TxScriptCacheCounters>,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync, S: BuildHasher + Default> Cache<TKey, TData, S> {
    pub fn new(size: u64) -> Self {
        Self::with_counters(size, Default::default())
    }

    pub fn with_counters(size: u64, counters: Arc<TxScriptCacheCounters>) -> Self {
        Self { map: Arc::new(HashCache::with_capacity_and_hasher(size as usize, size as usize, S::default())), counters }
    }

    pub(crate) fn get_or_insert(&self, key: TKey, v: impl FnOnce() -> TData) -> TData {
        let mut found = true;
        let res = self
            .map
            .entry(key)
            .or_put_with(|| {
                found = false;
                v()
            })
            .1
            .get()
            .clone();
        if found {
            self.counters.get_counts.fetch_add(1, Ordering::Relaxed);
        } else {
            self.counters.insert_counts.fetch_add(1, Ordering::Relaxed);
        }
        res
    }
    // pub(crate) fn get(&self, key: &TKey) -> Option<TData> {
    //     self.map.get(key).cloned().map(|data| {
    //         self.counters.get_counts.fetch_add(1, Ordering::Relaxed);
    //         data
    //     })
    // }
    //
    // pub(crate) fn insert(&self, key: TKey, data: TData) {
    //     _ = self.map.put(key, data);
    //             let mut write_guard = self.map.write();
    //     if write_guard.len() == self.size {
    //         write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
    //     }
    //     write_guard.insert(key, data);
    //     self.counters.insert_counts.fetch_add(1, Ordering::Relaxed);
    // }
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
            insert_counts: self.insert_counts.checked_sub(rhs.insert_counts).unwrap_or_default(),
            get_counts: self.get_counts.checked_sub(rhs.get_counts).unwrap_or_default(),
        }
    }
}
