use std::collections::{hash_map::Entry, HashMap};
use std::hash::Hash;

pub trait NestedHashMapExtensions<K, K2, V> {
    /// For use with a hashmap within a hashmap, takes an outer and inner key, and an inner value
    /// 1) if the outer key exists with and inner hashmap the key-value pair is inserted to the existing hashmap.
    /// 2) a new inner hashmap is created with the key-value pair if the outer key does not exist.
    fn insert_into_nested(&mut self, outer_key: K, inner_key: K2, inner_value: V);
}

impl<K, K2, V> NestedHashMapExtensions<K, K2, V> for HashMap<K, HashMap<K2, V>>
where
    K: Hash + PartialEq + Eq,
    K2: Hash + PartialEq + Eq,
{
    fn insert_into_nested(&mut self, outer_key: K, inner_key: K2, inner_value: V) {
        match self.entry(outer_key) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(inner_key, inner_value);
            }
            Entry::Vacant(entry) => {
                let mut inner = HashMap::with_capacity(1);
                // We do not expect duplicate insert since we just created the hashmap.
                let _ = &inner.insert(inner_key, inner_value);
                entry.insert(inner);
            }
        }
    }
}

pub trait GroupExtension<K, V, I>
where
    K: std::hash::Hash + Ord,
    I: IntoIterator<Item = (K, V)>,
{
    fn group_from(v: I) -> HashMap<K, Vec<V>>;
}

impl<K, V, I> GroupExtension<K, V, I> for HashMap<K, Vec<V>>
where
    K: std::hash::Hash + Ord,
    I: IntoIterator<Item = (K, V)>,
{
    fn group_from(v: I) -> HashMap<K, Vec<V>> {
        let mut result = HashMap::<K, Vec<V>>::new();
        for (a, b) in v {
            result.entry(a).or_default().push(b);
        }
        result
    }
}
