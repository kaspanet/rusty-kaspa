use ahash::{AHashMap, AHashSet};
use std::hash::Hash;
use std::collections::hash_map::Entry;


/// A AHashMap based "flush"-Counter.
/// 
/// Note: this counter flushes values when they reach zero. 
#[derive(Default, Debug, Clone)]
pub struct AHashCounter<T>(AHashMap<T, usize>);

impl <T> AHashCounter <T> 
where
    T: Hash + PartialEq + Eq + Clone,
{
    pub fn new() -> Self {
        Self{
            0: AHashMap::new()
        }
    }

    pub fn remove(&mut self, hashset: AHashSet<T>) -> AHashSet<T> {
        let mut removed = AHashSet::new();
        for k in hashset.into_iter() {
            match self.0.entry(k) {
                Entry::Occupied(mut entry) => {
                    if entry.insert(*entry.get() - 1) == 1 {
                        removed.insert(entry.remove_entry().0);
                    }
                },
                Entry::Vacant(_) => continue,
            };
        }
        removed
    }

    pub fn add(&mut self, hashset: AHashSet<T>) -> AHashSet<T> {
        let mut added = AHashSet::new();
        for k in hashset.into_iter() {
            match self.0.entry(k) {
                Entry::Occupied(mut entry) => {
                    entry.insert(*entry.get() + 1);
                },
                Entry::Vacant(entry) => {
                    added.insert(entry.key().clone());
                    entry.insert(1);
                }
            };
        }
        added
    }

    pub fn get_active_set(&mut self) -> AHashSet<T> {
        self.0.keys().cloned().collect()
    }
}