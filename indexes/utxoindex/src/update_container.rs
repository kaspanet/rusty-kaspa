use consensus_core::{
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
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

    /// Add a [`UtxoCollection`] the the [`UtxoIndexChanges`] struct
    ///
    /// Note: Always remove before add.
    pub fn add_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_outpoint, utxo_entry) in utxo_collection.into_iter() {
            match self.utxo_changes.removed.entry(utxo_entry.script_public_key.clone()) {
                Entry::Occupied(mut entry) => match entry.get_mut().entry(transaction_outpoint) {
                    Entry::Occupied(inner_entry) => {
                        inner_entry.remove_entry();
                    }
                    Entry::Vacant(_) => (),
                },
                Entry::Vacant(_) => (),
            };

            self.supply_change += utxo_entry.amount as i64; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxo_changes.added.entry(utxo_entry.script_public_key) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                }
                Entry::Vacant(entry) => {
                    let mut value = CompactUtxoCollection::new();
                    value.insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                    entry.insert(value);
                }
            };
        }
    }

    /// Remove a [`UtxoCollection`] the the [`UtxoIndexChanges`] struct
    ///
    /// Note: Always remove before add
    pub fn remove_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_outpoint, utxo_entry) in utxo_collection.into_iter() {
            self.supply_change -= utxo_entry.amount as i64; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxo_changes.removed.entry(utxo_entry.script_public_key.clone()) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                }
                Entry::Vacant(entry) => {
                    let mut value = CompactUtxoCollection::new();
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
                    let mut value = CompactUtxoCollection::new();
                    value.insert(
                        transaction_outpoint,
                        CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
                    );
                    entry.insert(value); //Future: `insert_entry`: https://doc.rust-lang.org/std/collections/hash_map/enum.Entry.html#method.insert_entry
                }
            }
        }
        self.supply_change = circulating_supply as CirculatingSupplyDiff;
    }

    pub fn add_tips(&mut self, tips: Vec<Hash>) {
        self.tips = BlockHashSet::from_iter(tips);
    }
}
