use async_channel::{unbounded, Receiver, Sender};
use hashes::{Hash, HASH_SIZE};
use rand::{seq::SliceRandom, Rng};

use consensus_core::{
    notify::{ConsensusNotification, PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification},
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
    BlockHashSet, HashMapCustomHasher,
};

use crate::model::{CirculatingSupply, CirculatingSupplyDiff};
use crate::test_helpers::struct_builders::*;

use super::struct_builders::*;

#[derive(Clone)]
pub struct VirtualChangeEmulator {
    pub utxo_collection: UtxoCollection,
    pub tips: BlockHashSet,
    pub circulating_supply: u64,
    pub virtual_state: VirtualChangeSetNotification,
    pub script_public_key_pool: Vec<ScriptPublicKey>,
}

impl VirtualChangeEmulator {
    pub fn new() -> Self {
        Self {
            utxo_collection: UtxoCollection::new(),
            virtual_state: VirtualChangeSetNotification::default(),
            script_public_key_pool: Vec::new(),
            tips: BlockHashSet::new(),
            circulating_supply: 0,
        }
    }

    pub fn fill_utxo_collection(&mut self, amount: usize, script_public_key_pool_size: usize) {
        self.script_public_key_pool.extend((0..script_public_key_pool_size).map(move |_| generate_random_script_public_key()));
        self.utxo_collection = generate_random_utxos(amount, self.script_public_key_pool.clone());
        for (_, utxo_entry) in self.utxo_collection.clone() {
            self.circulating_supply += utxo_entry.amount;
        }
        self.tips = BlockHashSet::from_iter(generate_new_tips(1));
    }

    pub fn change_virtual_state(&mut self, remove_amount: usize, add_amount: usize, tip_amount: usize) {
        let mut new_circulating_supply_diff: CirculatingSupplyDiff = 0;
        for (k, v) in self.utxo_collection.iter().take(remove_amount) {
            new_circulating_supply_diff -= v.amount as CirculatingSupplyDiff;
            self.virtual_state.virtual_utxo_diff.remove.insert(*k, v.clone());
        }

        self.utxo_collection.retain(|k, _| !self.virtual_state.virtual_utxo_diff.remove.contains_key(k));

        for (k, v) in generate_random_utxos(add_amount, self.script_public_key_pool.clone()).iter() {
            new_circulating_supply_diff += v.amount as CirculatingSupplyDiff;
            self.virtual_state.virtual_utxo_diff.add.insert(*k, v.clone());
            self.utxo_collection.insert(*k, v.clone());
        }

        let new_tips = generate_new_tips(tip_amount);

        self.virtual_state.virtual_parents = new_tips.clone();
        self.tips = BlockHashSet::from_iter(new_tips);

        if new_circulating_supply_diff > 0 {
            //force monotonic
            self.circulating_supply += new_circulating_supply_diff as CirculatingSupply;
        }

        self.virtual_state.virtual_selected_parent_blue_score = 0;
        self.virtual_state.virtual_daa_score = 0;
    }

    pub fn clear_virtual_state(&mut self) {
        self.virtual_state.virtual_utxo_diff.add = UtxoCollection::new();
        self.virtual_state.virtual_utxo_diff.remove = UtxoCollection::new();
        self.virtual_state.virtual_parents = Vec::new();
        self.virtual_state.virtual_selected_parent_blue_score = 0;
        self.virtual_state.virtual_daa_score = 0;
    }
}

impl Default for VirtualChangeEmulator {
    fn default() -> Self {
        Self::new()
    }
}

///TODO: remove when txscript is there, For preliminary testing purposes only!!
pub fn script_public_key_to_pseudo_address(script_public_key: ScriptPublicKey) -> String {
    //create pseudo address-string until tx script parsing is possible.
    let mut bytes = [0; 38];
    bytes[..2].copy_from_slice(&script_public_key.version().to_le_bytes());
    bytes[2..].copy_from_slice(script_public_key.script());
    String::from(&("kaspa:".to_owned() + std::str::from_utf8(&bytes).unwrap()))
}
