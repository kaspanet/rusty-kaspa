use crate::tx::*;
use std::collections::HashMap;

pub type UtxoCollection = HashMap<TransactionOutpoint, UtxoEntry>;

pub trait UtxoCollectionExtensions {
    /// Checks if the `outpoint` key exists with an entry that holds `entry.block_daa_score == daa_score`
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool;

    /// Adds all entries from `other` to `self`.
    /// Note that this means that values from `other` might override values of `self`.   
    fn add_many(&mut self, other: &Self);

    /// Removes all elements in `other` from `self`. Equivalent to `self - other` in set theory.
    fn remove_many(&mut self, other: &Self);

    /// Returns whether the intersection between the two collections is not empty.
    fn intersects(&self, other: &Self) -> bool;
}

impl UtxoCollectionExtensions for UtxoCollection {
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool {
        if let Some(entry) = self.get(outpoint) {
            entry.block_daa_score == daa_score
        } else {
            false
        }
    }

    fn add_many(&mut self, other: &Self) {
        for (k, v) in other.iter() {
            self.insert(*k, v.clone());
        }
    }

    fn remove_many(&mut self, other: &Self) {
        for k in other.keys() {
            self.remove(k);
        }
    }

    fn intersects(&self, other: &Self) -> bool {
        // We prefer iterating over the smaller set
        let (keys, other) = if self.len() <= other.len() { (self.keys(), other) } else { (other.keys(), self) };

        for k in keys {
            if other.contains_key(k) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_types() {
        let mut map = UtxoCollection::new();

        map.insert(
            TransactionOutpoint { transaction_id: 6.into(), index: 1 },
            UtxoEntry {
                amount: 5,
                script_public_key: Arc::new(ScriptPublicKey::default()),
                block_daa_score: 765,
                is_coinbase: false,
            },
        );
        dbg!(map);
    }
}
