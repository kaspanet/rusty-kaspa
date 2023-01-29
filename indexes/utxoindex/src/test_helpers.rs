use async_std::channel::{unbounded, Receiver, Sender};
use consensus_core::{
    notify::{ConsensusNotification, PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification},
    tx::{ScriptPublicKey, ScriptPublicKeys, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
    BlockHashSet, HashMapCustomHasher,
};
use hashes::{Hash, HASH_SIZE};
use rand::seq::SliceRandom;
use rand::Rng;

// TODO: this is an ineffecient, Ad-hoc testing helper / platform which emulates virtual changes with random bytes,
// remove all this, and rework testing when proper simulation is possible with test / sim consensus.
// Note: generated structs are generally filled with random bytes ad do not represent fully consensus conform and valid structs.

fn generate_random_utxos(amount: usize, script_public_key_pool: ScriptPublicKeys) -> UtxoCollection {
    let mut rng = rand::thread_rng();
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
fn generate_random_utxo(script_public_key_pool: ScriptPublicKeys) -> UtxoEntry {
    let mut rng = rand::thread_rng();
    UtxoEntry::new(
        rng.gen_range(1..100_000_000_000_000),
        Vec::from_iter(script_public_key_pool).choose(&mut rng).expect("expected_script_public key").clone(),
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

pub struct VirtualChangeEmulator {
    pub utxo_collection: UtxoCollection,
    pub tips: BlockHashSet,
    pub circulating_supply: u64,
    pub virtual_state: VirtualChangeSetNotification,
    pub script_public_key_pool: ScriptPublicKeys,
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
            script_public_key_pool: ScriptPublicKeys::new(),
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
        let mut to_remove = UtxoCollection::new();

        for (k, v) in self.utxo_collection.clone().into_iter().take(remove_amount) {
            self.circulating_supply -= v.amount;
            to_remove.insert(k, v);
            self.utxo_collection.remove(&k);
        }

        self.virtual_state.virtual_utxo_diff.remove.extend(to_remove);

        let mut to_add = UtxoCollection::new();
        for (k, v) in generate_random_utxos(add_amount, self.script_public_key_pool.clone()).iter() {
            self.circulating_supply += v.amount;
            to_add.insert(*k, v.clone());
        }

        self.utxo_collection.extend(to_add.clone());
        self.virtual_state.virtual_utxo_diff.add.extend(to_add);

        let new_tips = generate_new_tips(tip_amount);
        self.virtual_state.virtual_parents = generate_new_tips(tip_amount);
        self.tips = BlockHashSet::from_iter(new_tips);

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
            .try_send(ConsensusNotification::PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification {}))
            .expect("expected send");
    }
    pub fn clear_virtual_state(&mut self) {
        self.virtual_state = VirtualChangeSetNotification::default();
    }
}
