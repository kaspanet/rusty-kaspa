#[cfg(test)]
use rand::Rng;
use std::sync::Arc;

use consensus::test_helpers::*;

use consensus_core::{
    events::VirtualChangeSetEvent,
    tx::ScriptPublicKey,
    utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
    BlockHashSet, HashMapCustomHasher,
};

use utxoindex::model::{CirculatingSupply, CirculatingSupplyDiff};

pub struct VirtualChangeEmulator {
    pub utxo_collection: UtxoCollection,
    pub tips: BlockHashSet,
    pub circulating_supply: u64,
    pub virtual_state: VirtualChangeSetEvent,
    pub script_public_key_pool: Vec<ScriptPublicKey>,
}

impl VirtualChangeEmulator {
    pub fn new() -> Self {
        Self {
            utxo_collection: UtxoCollection::new(),
            virtual_state: VirtualChangeSetEvent::default(),
            script_public_key_pool: Vec::new(),
            tips: BlockHashSet::new(),
            circulating_supply: 0,
        }
    }

    pub fn fill_utxo_collection(&mut self, amount: usize, script_public_key_pool_size: usize) {
        let rng = &mut rand::thread_rng();
        self.script_public_key_pool
            .extend((0..script_public_key_pool_size).map(|_| generate_random_p2pk_script_public_key(&mut rng.clone())));
        self.utxo_collection =
            generate_random_utxos_from_script_public_key_pool(&mut rng.clone(), amount, self.script_public_key_pool.clone());
        for (_, utxo_entry) in self.utxo_collection.clone() {
            self.circulating_supply += utxo_entry.amount;
        }
        self.tips = BlockHashSet::from_iter(generate_random_hashes(&mut rng.clone(), 1));
    }

    pub fn change_virtual_state(&mut self, remove_amount: usize, add_amount: usize, tip_amount: usize) {
        let rng = &mut rand::thread_rng();

        let mut new_circulating_supply_diff: CirculatingSupplyDiff = 0;
        self.virtual_state.selected_parent_utxo_diff = Arc::new(UtxoDiff::new(
            UtxoCollection::from_iter(
                generate_random_utxos_from_script_public_key_pool(&mut rng.clone(), add_amount, self.script_public_key_pool.clone())
                    .into_iter()
                    .map(|(k, v)| {
                        new_circulating_supply_diff += v.amount as CirculatingSupplyDiff;
                        (k, v)
                    }),
            ),
            UtxoCollection::from_iter(self.utxo_collection.iter().take(remove_amount).map(|(k, v)| {
                new_circulating_supply_diff -= v.amount as CirculatingSupplyDiff;
                (*k, v.clone())
            })),
        ));

        self.utxo_collection.retain(|k, _| !self.virtual_state.selected_parent_utxo_diff.remove.contains_key(k));
        self.utxo_collection.extend(self.virtual_state.selected_parent_utxo_diff.add.iter().map(move |(k, v)| (*k, v.clone())));

        let new_tips = Arc::new(generate_random_hashes(&mut rng.clone(), tip_amount));

        self.virtual_state.parents = new_tips.clone();
        self.tips = BlockHashSet::from_iter(new_tips.iter().cloned());

        // Force monotonic
        if new_circulating_supply_diff > 0 {
            self.circulating_supply += new_circulating_supply_diff as CirculatingSupply;
        }

        self.virtual_state.selected_parent_blue_score = rng.gen();
        self.virtual_state.daa_score = rng.gen();
    }

    pub fn clear_virtual_state(&mut self) {
        self.virtual_state.selected_parent_utxo_diff = Arc::new(UtxoDiff::new(UtxoCollection::new(), UtxoCollection::new()));

        self.virtual_state.parents = Arc::new(Vec::new());
        self.virtual_state.selected_parent_blue_score = 0;
        self.virtual_state.daa_score = 0;
    }
}

impl Default for VirtualChangeEmulator {
    fn default() -> Self {
        Self::new()
    }
}
