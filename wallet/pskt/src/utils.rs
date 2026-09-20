//! Utility functions for the PSKT module.

use std::collections::BTreeMap;

// todo optimize without cloning
pub fn combine_if_no_conflicts<K, V>(mut lhs: BTreeMap<K, V>, rhs: BTreeMap<K, V>) -> Result<BTreeMap<K, V>, Error<K, V>>
where
    V: Eq + Clone,
    K: Ord + Clone,
{
    if lhs.len() >= rhs.len() {
        if let Some((field, rhs, lhs)) =
            rhs.iter().map(|(k, v)| (k, v, lhs.get(k))).find(|(_, v, rhs_v)| rhs_v.is_some_and(|rv| rv != *v))
        {
            Err(Error { field: field.clone(), lhs: lhs.unwrap().clone(), rhs: rhs.clone() })
        } else {
            lhs.extend(rhs);
            Ok(lhs)
        }
    } else {
        combine_if_no_conflicts(rhs, lhs)
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("Conflict")]
pub struct Error<K, V> {
    pub field: K,
    pub lhs: V,
    pub rhs: V,
}
