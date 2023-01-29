use async_std::channel::{unbounded, Receiver, Sender};
use consensus_core::{
    notify::{ConsensusNotification, PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification},
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
    BlockHashSet, HashMapCustomHasher,
};
use hashes::{Hash, HASH_SIZE};
use rand::seq::SliceRandom;
use rand::Rng;

use crate::external::model::{CirculatingSupply, CirculatingSupplyDiff};

// TODO: this is an ineffecient, Ad-hoc testing helper / platform which emulates virtual changes with random bytes,
// remove all this, and rework testing when proper simulation is possible with test / sim consensus.
// Note: generated structs are generally filled with random bytes ad do not represent fully consensus conform and valid structs.

fn generate_random_utxos(amount: usize, script_public_key_pool: Vec<ScriptPublicKey>) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection.insert(generate_random_outpoint(), generate_random_utxo(script_public_key_pool.clone()));
        i += 1;
    }
    collection
}

fn generate_random_hash() -> Hash {
    let random_bytes = rand::thread_rng().gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
fn generate_random_outpoint() -> TransactionOutpoint {
    let mut rng = rand::thread_rng();
    TransactionOutpoint::new(generate_random_hash(), rng.gen::<u32>())
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
fn generate_random_utxo(script_public_key_pool: Vec<ScriptPublicKey>) -> UtxoEntry {
    let mut rng = rand::thread_rng();
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(&mut rng).expect("expected_script_public key").clone(),
        rng.gen_range(1..100_000_000),
        rng.gen_bool(0.1),
    )
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
fn generate_random_script_public_key() -> ScriptPublicKey {
    let script: ScriptVec = (0..36).map(|_| rand::random::<u8>()).collect();
    ScriptPublicKey::new(0 as u16, script)
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
fn generate_new_tips(amount: usize) -> Vec<Hash> {
    let mut tips = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        tips.push(generate_random_hash());
        i += 1;
    }
    tips
}

#[derive(Clone)]
pub struct VirtualChangeEmulator {
    pub utxo_collection: UtxoCollection,
    pub tips: BlockHashSet,
    pub circulating_supply: u64,
    pub virtual_state: VirtualChangeSetNotification,
    pub script_public_key_pool: Vec<ScriptPublicKey>,
    sender: Sender<ConsensusNotification>,
    pub receiver: Receiver<ConsensusNotification>,
}

impl VirtualChangeEmulator {
    pub fn new() -> Self {
        let (s, r) = unbounded::<ConsensusNotification>();
        Self {
            utxo_collection: UtxoCollection::new(),
            virtual_state: VirtualChangeSetNotification::default(),
            sender: s,
            receiver: r,
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

    pub fn signal_intial_state(&mut self) {
        self.virtual_state.virtual_utxo_diff.add = self.utxo_collection.clone();
        self.virtual_state.virtual_parents = generate_new_tips(1);
        self.sender.try_send(ConsensusNotification::VirtualChangeSet(self.virtual_state.clone())).expect("expected send");
    }

    pub fn signal_virtual_state(&self) {
        self.sender.try_send(ConsensusNotification::VirtualChangeSet(self.virtual_state.clone())).expect("expected send");
    }

    pub fn signal_utxoset_override(&self) {
        self.sender
            .try_send(ConsensusNotification::PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification::new()))
            .expect("expected send");
    }
    pub fn clear_virtual_state(&mut self) {
        self.virtual_state.virtual_utxo_diff.add = UtxoCollection::new();
        self.virtual_state.virtual_utxo_diff.remove = UtxoCollection::new();
        self.virtual_state.virtual_parents = Vec::new();
        self.virtual_state.virtual_selected_parent_blue_score = 0;
        self.virtual_state.virtual_daa_score = 0;
    }
}
