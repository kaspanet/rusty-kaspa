use consensus::model::stores::virtual_state::{VirtualState};
use consensus_core::BlockHashSet;
use consensus_core::tx::{UtxoEntry, TransactionOutpoint};
use consensus_core::{utxo::utxo_collection::UtxoCollection};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::collections::hash_map::Entry;
use hashes::Hash;

use crate::model::CompactUtxoEntry;

use super::UtxoSetDiffByScriptPublicKey;

/// A struct holding all changes to the utxo index.
pub struct UtxoIndexChanges {
    pub utxo_diff: UtxoSetDiffByScriptPublicKey,
    pub circulating_supply_diff: i64,
    pub tips: BlockHashSet
}

impl UtxoIndexChanges {
    pub fn new() -> Self {
        Self { 
            utxo_diff: UtxoSetDiffByScriptPublicKey::new(),
            circulating_supply_diff: 0, 
            tips: BlockHashSet::new(),
        }
    }

    pub fn add_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_output, utxo_entry) in utxo_collection.into_iter() {
            
            match self.utxo_diff.removed.entry(utxo_entry.script_public_key) {
                Entry::Occupied(entry) => { 
                    entry.remove_entry();
                    continue;
                },
                Entry::Vacant(entry) => {},
            }
                        
            self.circulating_supply_diff += utxo_entry.amount as i64;
            
            match self.utxo_diff.added.entry(utxo_entry.script_pub_key) {
                Entry::Occupied(entry) => { 
                    entry.get_mut().insert(transaction_output, utxo_entry.into()).expect("expected no duplicate utxo entries")
                },
                Entry::Vacant(entry) => {
                    let inner_entry = HashMap::new::<TransactionOutpoint, CompactUtxoEntry>();
                    entry.insert_entry(inner_entry.insert(transaction_output, utxo_entry.into()).expect("expected no duplicate utxo entries"));
                },
            }
        }
    }

    pub fn remove_utxo_collection(&mut self, utxo_collection: UtxoCollection) {
        for (transaction_output, utxo_entry) in utxo_collection.into_iter() {
            
            self.circulating_supply_diff -= utxo_entry.amount as i64;

            match self.utxo_diff.removed.entry(utxo_entry.script_pub_key) {
                Entry::Occupied(entry) => { 
                    entry.get_mut().insert(transaction_output, utxo_entry.into()).expect("expected no duplicate utxo entries");
                },
                Entry::Vacant(entry) => {
                    let inner_entry = HashMap::new::<TransactionOutpoint, CompactUtxoEntry>();
                    entry.insert_entry(inner_entry.insert(transaction_output, utxo_entry.into()).expect("expected no duplicate utxo entries"));
                },
            };
        }
    }

    //used when resetting, since we don't access a collection.
    pub fn add_utxo(&mut self, tx_out: TransactionOutpoint, utxo_entry: UtxoEntry) {
        self.circulating_supply_diff += utxo_entry.amount;
        match self.utxo_diff.added.entry(utxo_entry.script_public_key) {
            Entry::Occupied(entry) => { 
                entry.get_mut().insert(tx_out, utxo_entry.into()).expect("expected no duplicate utxo entries"); 
            },
            Entry::Vacant(entry) => {
                let inner_entry = HashMap::new::<TransactionOutpoint, CompactUtxoEntry>();
                entry.insert_entry(inner_entry.insert(tx_out, utxo_entry.into()).expect("expected no duplicate utxo entries"));
                }
        }
    }

    pub fn add_tips(&mut self, tips: Vec<Hash>) -> bool {
        self.tips = BlockHashSet::from_iter(tips);
    }
}

impl From<VirtualState> for UtxoIndexChanges {
    fn from(virtual_state: VirtualState) -> Self {
        let mut sel = Self::new();
        sel.remove_utxo_collection(virtual_state.utxo_diff.remove);
        sel.add_utxo_collection(virtual_state.utxo_diff.add);
        sel.add_tips(virtual_state.parents);
        sel
    }
}
