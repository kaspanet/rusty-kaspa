use consensus::model::stores::virtual_state::{self, VirtualState};
use consensus_core::tx::UtxoEntry;
use consensus_core::{tx::TransactionOutpoint, utxo::utxo_collection::UtxoCollection};
use std::collections::hash_map::Iter;
use std::collections::{HashMap, HashSet};
use std::iter::Chain;
use hashes::Hash;

use super::*;

/// A struct holding all changes to the utxo index.
pub struct UtxoIndexChanges {
    pub utxo_diff: UtxoSetDiffByScriptPublicKey,
    pub circulating_supply_diff: i64,
    pub tips: HashSet<Hash>
}

impl UtxoIndexChanges {
    pub fn new() -> Self {
        Self { 
            utxo_diff: UtxoSetDiffByScriptPublicKey::new(),
            circulating_supply_diff: 0, 
            tips: HashSet::new()
        }
    }

    pub fn add_utxo_collection(&mut self, utxo_collection: UtxoCollection) -> bool {
        for (transaction_output, utxo) in utxo_collection.into_iter() {
            
            let script_pub_key = utxo.script_public_key;
            
            let compact_utxo = CompactUtxoEntry::from(utxo);
            
            self.circulating_supply_diff += compact_utxo.amount as i64;
            //For Future: check if https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert can be utilized
            let compact_collection = CompactUtxoCollection::new();
            
            match self.utxo_diff.removed.get_mut(&script_pub_key) { //Try and remove from self.remove, and continue if removed from self.remove. 
                Some(val) => match val.remove(&transaction_output) {
                    Some(val) => continue,
                    None => (),
                }
                None => (),
            }

            compact_collection.insert(transaction_output, putxo).unwrap(); //TODO: error handle
            match self.utxo_diff.added.insert(script_pub_key, compact_collection) {
                Some(compact_collection) => {
                    compact_collection.insert(transaction_output, compact_utxo).unwrap();
                }
                _ => (),
            }
        }
    }

    pub fn remove_utxo_collection(&mut self, utxo_collection: UtxoCollection) -> bool {
        for (transaction_output, utxo) in utxo_collection.into_iter() {
            
            let script_pub_key = utxo.script_public_key;
            
            let compact_utxo = CompactUtxoEntry::from(utxo);
            
            self.circulating_supply_diff -= compact_utxo.amount as i64;
            //For Future: check if https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert can be utilized
            let compact_collection = CompactUtxoCollection::new();

            compact_collection.insert(transaction_output, putxo).unwrap(); //TODO: error handle
            match self.utxo_diff.removed.insert(script_pub_key, compact_collection) {
                Some(compact_collection) => {
                    compact_collection.insert(transaction_output, compact_utxo).unwrap();
                }
                _ => (),
            }
        }

    pub fn add_tips(&mut self, tips: Vec<Hash>) -> bool {
        self.tips = HashMap::from_iter(tips);
    }

    pub fn add_utxo(&mut self, utxo_collection: UtxoEntry) -> bool {
        for (transaction_output, utxo) in utxo_collection.into_iter() {
            
            let script_pub_key = utxo.script_public_key;
            
            let compact_utxo = CompactUtxoEntry::from(utxo);
            
            self.circulating_supply_diff += compact_utxo.amount as i64;
            //For Future: check if https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert can be utilized
            let compact_collection = CompactUtxoCollection::new();
            
            match self.utxo_diff.removed.get_mut(&script_pub_key) { //Try and remove from self.remove, and continue if removed from self.remove. 
                Some(val) => match val.remove(&transaction_output) {
                    Some(val) => continue,
                    None => (),
                }
                None => (),
            }

            compact_collection.insert(transaction_output, putxo).unwrap(); //TODO: error handle
            match self.utxo_diff.added.insert(script_pub_key, compact_collection) {
                Some(compact_collection) => {
                    compact_collection.insert(transaction_output, compact_utxo).unwrap();
                }
                _ => (),
            }
        }
    }
}
}


impl From<VirtualState> for UtxoSetDiffByScriptPublicKey {
    fn from(virtual_state: VirtualState) -> Self {
        let mut sel = Self::new();
        sel.remove_utxo_collection(virtual_state.utxo_diff.remove);
        sel.add_utxo_collection(virtual_state.utxo_diff.add);
        sel.add_tips(virtual_state.parents);
        sel
    }
}