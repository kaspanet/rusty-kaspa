use consensus_core::notify::VirtualChangeSetNotification;
use consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use consensus_core::utxo::utxo_collection::UtxoCollection;
use consensus_core::{BlockHashSet, HashMapCustomHasher};
use hashes::Hash;
use std::collections::hash_map::Entry;

use crate::external::model::{CirculatingSupplyDiff, CompactUtxoCollection, CompactUtxoEntry, UtxoSetByScriptPublicKey};
use crate::external::notify::UtxosChangedNotification;
use crate::model::CirculatingSupply;

///A struct holding UTXO changes to the utxoindex.
#[derive(Debug, Clone)]
pub struct UTXOChanges {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UTXOChanges {
    pub fn new(added: UtxoSetByScriptPublicKey, removed: UtxoSetByScriptPublicKey) -> Self {
        Self { added, removed }
    }
}

/// A struct holding all changes to the utxoindex.
/// changes are computed as entries are inserted or conversion from virtual changed set happens.
pub struct UtxoIndexChanges {
    pub utxos: UTXOChanges,
    pub supply: CirculatingSupplyDiff,
    pub tips: BlockHashSet,
}

impl UtxoIndexChanges {
    ///create a new [`UtxoIndexChanges`] struct
    pub fn new() -> Self {
        Self {
            utxos: UTXOChanges::new(UtxoSetByScriptPublicKey::new(), UtxoSetByScriptPublicKey::new()),
            supply: 0,
            tips: BlockHashSet::new(),
        }
    }

    ///create a new [`UtxoIndexChanges`] struct
    pub fn add_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_outpoint, utxo_entry) in utxo_collection.into_iter() {
            match self.utxos.removed.entry(utxo_entry.script_public_key.clone()) {
                Entry::Occupied(mut entry) => match entry.get_mut().entry(transaction_outpoint) {
                    Entry::Occupied(inner_entry) => {
                        inner_entry.remove_entry();
                    }
                    Entry::Vacant(_) => (),
                },
                Entry::Vacant(_) => (),
            };

            self.supply += utxo_entry.amount as i64; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxos.added.entry(utxo_entry.script_public_key) {
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

    pub fn remove_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_outpoint, utxo_entry) in utxo_collection.into_iter() {
            self.supply -= utxo_entry.amount as i64; // TODO: Using `virtual_state.mergeset_rewards` might be a better way to extract this.

            match self.utxos.removed.entry(utxo_entry.script_public_key.clone()) {
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

    pub fn add_utxo_vector(&mut self, utxo_vector: Vec<(TransactionOutpoint, UtxoEntry)>) {
        let mut circulating_supply: CirculatingSupply = 0;

        for (transaction_outpoint, utxo_entry) in utxo_vector.into_iter() {
            circulating_supply += utxo_entry.amount;

            match self.utxos.added.entry(utxo_entry.script_public_key) {
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
        self.supply = circulating_supply as CirculatingSupplyDiff;
    }

    pub fn add_tips(&mut self, tips: Vec<Hash>) {
        self.tips = BlockHashSet::from_iter(tips);
    }
}

impl From<VirtualChangeSetNotification> for UtxoIndexChanges {
    fn from(virtual_change_set: VirtualChangeSetNotification) -> Self {
        let mut _self = Self::new();
        _self.remove_utxo_collection(virtual_change_set.virtual_utxo_diff.remove);
        _self.add_utxo_collection(virtual_change_set.virtual_utxo_diff.add);
        _self.add_tips(virtual_change_set.virtual_parents);
        _self
    }
}

impl From<UTXOChanges> for UtxosChangedNotification {
    fn from(utxo_changes: UTXOChanges) -> Self {
        Self { added: utxo_changes.added, removed: utxo_changes.removed }
    }
}
