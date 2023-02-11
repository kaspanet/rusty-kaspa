use consensus_core::{
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_diff::UtxoDiff,
    BlockHashSet, HashMapCustomHasher,
};
use hashes::Hash;
use std::collections::hash_map::Entry;

use crate::model::{
    CirculatingSupply, CirculatingSupplyDiff, CompactUtxoCollection, CompactUtxoEntry, UtxoChanges, UtxoSetByScriptPublicKey,
};

/// A struct holding all changes to the utxoindex with on-the-fly conversions and processing.
pub struct UtxoIndexChanges {
    pub utxo_changes: UtxoChanges,
    pub supply_change: CirculatingSupplyDiff,
    pub tips: BlockHashSet,
}

impl UtxoIndexChanges {
    /// Create a new [`UtxoIndexChanges`] struct
    pub fn new() -> Self {
        Self {
            utxo_changes: UtxoChanges::new(UtxoSetByScriptPublicKey::new(), UtxoSetByScriptPublicKey::new()),
            supply_change: 0,
            tips: BlockHashSet::new(),
        }
    }

    /// Add a [`UtxoDiff`] the the [`UtxoIndexChanges`] struct.
    pub fn add_utxo_diff(&mut self, utxo_diff: UtxoDiff) {
        let (to_add, mut to_remove) = (utxo_diff.add, utxo_diff.remove);

        for (transaction_outpoint, utxo_entry) in to_add.into_iter() {
            to_remove.remove(&transaction_outpoint); // We try and remove from `utxo_diff.remove`.

            self.supply_change += utxo_entry.amount as CirculatingSupplyDiff; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxo_changes.added.entry(utxo_entry.script_public_key) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                }
                Entry::Vacant(entry) => {
                    let mut value = CompactUtxoCollection::with_capacity(1);
                    value.insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                    entry.insert(value);
                }
            };
        }

        for (transaction_outpoint, utxo_entry) in to_remove.into_iter() {
            self.supply_change -= utxo_entry.amount as CirculatingSupplyDiff; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxo_changes.removed.entry(utxo_entry.script_public_key) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                }
                Entry::Vacant(entry) => {
                    let mut value = CompactUtxoCollection::with_capacity(1);
                    value.insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                    entry.insert(value);
                }
            };
        }
    }

    /// Add a [`Vec<(TransactionOutpoint, UtxoEntry)>`] the the [`UtxoIndexChanges`] struct
    ///
    /// Note: This is meant to be used when resyncing.
    pub fn add_utxo_collection_vector(&mut self, utxo_vector: Vec<(TransactionOutpoint, UtxoEntry)>) {
        let mut circulating_supply: CirculatingSupply = 0;

        for (transaction_outpoint, utxo_entry) in utxo_vector.into_iter() {
            circulating_supply += utxo_entry.amount;

            match self.utxo_changes.added.entry(utxo_entry.script_public_key) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                }
                Entry::Vacant(entry) => {
                    let mut value = CompactUtxoCollection::with_capacity(1);
                    value.insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                    entry.insert(value);
                }
            }
        }
        self.supply_change = circulating_supply as CirculatingSupplyDiff;
    }

    pub fn add_tips(&mut self, tips: Vec<Hash>) {
        self.tips = BlockHashSet::from_iter(tips);
    }
}
