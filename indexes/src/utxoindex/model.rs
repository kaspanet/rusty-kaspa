use std::collections::{HashMap, HashSet};

use consensus::model::stores::virtual_state::VirtualState;
use consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use consensus_core::utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff};
use consensus_core::blockhash::BlockHashes;

pub type Tips = BlockHashes;

//TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
//One possible implementation: u64 from 8 bytes of of transaction id xored with 4 bytes of transaction index.
pub type UtxoIndexedUtxoCollection = HashMap<TransactionOutpoint, UtxoIndexedUtxoEntry>;

//TODO: same as above for txs with OP Pushed unique data,
//One possible implementation: Extraction of unique data segment
pub type UtxosByScriptPublicKey = HashMap<ScriptPublicKey, UtxoIndexedUtxoCollection>;

//Note: memory optimizaion compared to go-lang kaspad:
//unlike `consensus_core::tx::UtxoEntry` the`script_public_key` field is removed (as they are keyed via `UtxosByScriptPublicKey`).
#[derive(Clone, Eq)]
pub struct UtxoIndexedUtxoEntry {
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl From<UtxoEntry> for UtxoIndexedUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self { amount: utxo_entry.amount, block_daa_score: utxo_entry.block_daa_score, is_coinbase: utxo_entry.is_coinbase }
    }
}

impl From<UtxoCollection> for UtxosByScriptPublicKey {
    fn from(utxo_collection: UtxoCollection) -> Self {
        utxo_collection.into_iter().for_each(|(transaction_output, utxo)| -> (ScriptPublicKey, UtxoIndexedUtxoCollection) {
            let putxo = UtxoIndexedUtxoEntry::from(utxo);
            //For Future: https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert
            match Self::insert(Self, utxo.script_public_key, UtxoIndexedUtxoCollection::from_iter((transaction_output, putxo))) {
                Some(partial_utxo_collection) => {
                    partial_utxo_collection.insert(transaction_output, UtxoIndexedUtxoEntry::from(utxo)).unwrap();
                }
                None => (),
            }
        });
    }
}

impl From<UtxoCollection> for UtxoIndexedUtxoCollection {
    fn from(utxo_collection: UtxoCollection) -> Self {
        Self::from_iter(
            utxo_collection
                .into_iter()
                .for_each(|(k, v)| -> (TransactionOutpoint, UtxoIndexedUtxoEntry) { (k, UtxoIndexedUtxoEntry::from((k, v))) }),
        )
    }
}

struct UtxoIndexUtxoDiff {
    add: UtxosByScriptPublicKey,
    remove: UtxosByScriptPublicKey,
}
pub struct UtxoIndexDiff {
    utxo_index_utxo_diff: UtxoIndexUtxoDiff,
    sompi_change_amount: i128, //Note: can't be u64 because value can go negative (fee deduction paid in next block), and can't be i64 because it won't cover max sompi amount. 
    tips: Tips,
}


impl UtxoIndexDiff {

    pub fn add_utxo_collection(mut self, to_add: UtxoCollection) {
        to_add
        .into_iter()
        .for_each(|(transaction_output, utxo)| {
            let putxo = UtxoIndexedUtxoEntry::from(utxo);
            //For Future: https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert
            self.sompi_change_amount += putxo.amount;
            match self.utxo_index_utxo_diff.add.insert(
                utxo.script_public_key, UtxoIndexedUtxoCollection::from_iter((transaction_output, putxo))
            ) {
                Some(partial_utxo_collection) => {
                    partial_utxo_collection.insert(transaction_output, putxo).unwrap();
                }
                None => _,
            }
        }
        );    
}
    pub fn remove_utxo_collection(mut self, to_remove: UtxoCollection) {
        to_remove
        .into_iter()
        .for_each(|(transaction_output, utxo)| {
            let putxo = UtxoIndexedUtxoEntry::from(utxo);
            //For Future: https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert
            self.sompi_change_amount -= putxo.amount;
            match self.utxo_index_utxo_diff.remove.insert(
                utxo.script_public_key, UtxoIndexedUtxoCollection::from_iter((transaction_output, putxo))
            ) {
                Some(partial_utxo_collection) => {
                    partial_utxo_collection.insert(transaction_output, putxo).unwrap();
                }
                None => _,
            }
        }
        )
    }

    pub fn clear(mut self) {
        self.utxo_index_utxo_diff.add.clear();
        self.utxo_index_utxo_diff.remove.clear();
        self.sompi_change_amount = 0;
    }

    pub fn update(mut self, utxo_diff: UtxoDiff) {
        self.add(utxo_diff.add);
        self.remove(utxo_diff.remove);
    }
}

impl From<VirtualState> for UtxoIndexDiff {
    fn from(virtual_state: VirtualState) -> Self {
        Self{
            add_utxo_collection(virtual_state.utxo_diff.add),
            remove_utxo_collection(virtual_state.utxo_diff.remove),
            tips: Tips::from_iter(virtual_state.parents),
    }
}


}
pub enum UtxoIndexEventType{
    UtxoChangeType
}

pub enum UtxoIndexEvent{
    UtxoChange
}

pub type Listeners = HashMap<UtxoIndexEventType, Sender<UtxoIndexEvent>>;