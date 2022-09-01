use crate::tx::*;
use std::collections::HashMap;

pub type UtxoCollection = HashMap<TransactionOutpoint, UtxoEntry>;

pub trait UtxoCollectionExtensions {
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool;
    fn add_set(&mut self, other: &Self);
    fn remove_set(&mut self, other: &Self);
}

impl UtxoCollectionExtensions for UtxoCollection {
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool {
        if let Some(entry) = self.get(outpoint) {
            entry.block_daa_score == daa_score
        } else {
            false
        }
    }

    fn add_set(&mut self, other: &Self) {
        for (k, v) in other.iter() {
            self.insert(*k, v.clone());
        }
    }

    fn remove_set(&mut self, other: &Self) {
        for (k, _) in other.iter() {
            self.remove(k);
        }
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
