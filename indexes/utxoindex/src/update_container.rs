use kaspa_consensus_core::{
    BlockHashSet, HashMapCustomHasher,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_diff::UtxoDiff,
};
use kaspa_hashes::Hash;
use kaspa_index_core::indexed_utxos::{CovenantUtxoChanges, UtxoEntryKeySuffixRecord};
use kaspa_utils::hashmap::NestedHashMapExtensions;

use crate::model::{CirculatingSupplyDiff, CompactUtxoEntry, UtxoChanges, UtxoSetByScriptPublicKey};

/// A struct holding all changes to the utxoindex with on-the-fly conversions and processing.
pub struct UtxoIndexChanges {
    pub utxo_changes: UtxoChanges,
    pub covenant_utxo_changes: CovenantUtxoChanges,
    pub supply_change: CirculatingSupplyDiff,
    pub tips: BlockHashSet,
}

impl UtxoIndexChanges {
    /// Create a new [`UtxoIndexChanges`] struct
    pub fn new() -> Self {
        Self {
            utxo_changes: UtxoChanges::new(UtxoSetByScriptPublicKey::new(), UtxoSetByScriptPublicKey::new()),
            covenant_utxo_changes: CovenantUtxoChanges::new(),
            supply_change: 0,
            tips: BlockHashSet::new(),
        }
    }

    /// Add a [`UtxoDiff`] the [`UtxoIndexChanges`] struct. When `covenant_index` is set,
    /// covenant-keyed mirror sets are collected for the covenant index store.
    pub fn update_utxo_diff(&mut self, utxo_diff: UtxoDiff, covenant_index: bool) {
        let (to_add, to_remove) = (utxo_diff.add, utxo_diff.remove);

        for (transaction_outpoint, utxo_entry) in to_add.into_iter() {
            self.supply_change += utxo_entry.amount as CirculatingSupplyDiff;
            self.insert_added(transaction_outpoint, utxo_entry, covenant_index);
        }

        for (transaction_outpoint, utxo_entry) in to_remove.into_iter() {
            self.supply_change -= utxo_entry.amount as CirculatingSupplyDiff;
            self.insert_removed(transaction_outpoint, utxo_entry, covenant_index);
        }
    }

    fn insert_added(&mut self, transaction_outpoint: TransactionOutpoint, utxo_entry: UtxoEntry, covenant_index: bool) {
        let UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase, covenant_id } = utxo_entry;
        self.utxo_changes.added.insert_into_nested(
            script_public_key.clone(),
            UtxoEntryKeySuffixRecord::new(block_daa_score, transaction_outpoint),
            CompactUtxoEntry::new(amount, is_coinbase, covenant_id),
        );
        if covenant_index && let Some(covenant_id) = covenant_id {
            self.covenant_utxo_changes
                .added
                .entry(covenant_id)
                .or_default()
                .push((script_public_key, UtxoEntryKeySuffixRecord::new(block_daa_score, transaction_outpoint)));
        }
    }

    fn insert_removed(&mut self, transaction_outpoint: TransactionOutpoint, utxo_entry: UtxoEntry, covenant_index: bool) {
        let UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase, covenant_id } = utxo_entry;
        self.utxo_changes.removed.insert_into_nested(
            script_public_key.clone(),
            UtxoEntryKeySuffixRecord::new(block_daa_score, transaction_outpoint),
            CompactUtxoEntry::new(amount, is_coinbase, covenant_id),
        );
        if covenant_index && let Some(covenant_id) = covenant_id {
            self.covenant_utxo_changes
                .removed
                .entry(covenant_id)
                .or_default()
                .push((script_public_key, UtxoEntryKeySuffixRecord::new(block_daa_score, transaction_outpoint)));
        }
    }

    /// Add a [`Vec<(TransactionOutpoint, UtxoEntry)>`] the [`UtxoIndexChanges`] struct
    ///
    /// Note: This is meant to be used when resyncing.
    pub fn add_utxos_from_vector(&mut self, utxo_vector: Vec<(TransactionOutpoint, UtxoEntry)>, covenant_index: bool) {
        for (transaction_outpoint, utxo_entry) in utxo_vector.into_iter() {
            self.supply_change += utxo_entry.amount as CirculatingSupplyDiff;
            self.insert_added(transaction_outpoint, utxo_entry, covenant_index);
        }
    }

    pub fn set_tips(&mut self, tips: Vec<Hash>) {
        self.tips = BlockHashSet::from_iter(tips);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
    use kaspa_consensus_core::utxo::utxo_collection::UtxoCollection;

    fn utxo_entry(amount: u64, daa_score: u64, covenant_id: Option<Hash>) -> UtxoEntry {
        UtxoEntry::new(amount, ScriptPublicKey::from_vec(0, vec![0x01]), daa_score, false, covenant_id)
    }

    fn make_diff(id_a: Hash, id_b: Hash) -> UtxoDiff {
        UtxoDiff::new(
            UtxoCollection::from_iter([
                (TransactionOutpoint::new(Hash::from_u64_word(1), 0), utxo_entry(100, 1, Some(id_a))),
                (TransactionOutpoint::new(Hash::from_u64_word(2), 0), utxo_entry(200, 1, None)),
            ]),
            UtxoCollection::from_iter([(TransactionOutpoint::new(Hash::from_u64_word(3), 0), utxo_entry(300, 2, Some(id_b)))]),
        )
    }

    #[test]
    fn test_covenant_changes_built_from_diff() {
        let id_a = Hash::from_slice(&[1u8; 32]);
        let id_b = Hash::from_slice(&[2u8; 32]);

        let mut changes = UtxoIndexChanges::new();
        changes.update_utxo_diff(make_diff(id_a, id_b), true);
        assert_eq!(changes.covenant_utxo_changes.added.keys().copied().collect::<Vec<_>>(), vec![id_a]);
        assert_eq!(changes.covenant_utxo_changes.removed.keys().copied().collect::<Vec<_>>(), vec![id_b]);
        let (script_public_key, suffix) = &changes.covenant_utxo_changes.added[&id_a][0];
        assert_eq!(*script_public_key, ScriptPublicKey::from_vec(0, vec![0x01]));
        assert_eq!(suffix.daa_score(), 1);

        let mut changes = UtxoIndexChanges::new();
        changes.update_utxo_diff(make_diff(id_a, id_b), false);
        assert!(changes.covenant_utxo_changes.added.is_empty());
        assert!(changes.covenant_utxo_changes.removed.is_empty());
    }
}
