use async_std::channel::{unbounded, Receiver, Sender};
use consensus_core::{
    notify::{ConsensusNotification, PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification},
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use hashes::{Hash, HASH_SIZE};
use rand::Rng;

//TODO: this is an Ad hoc testing helper / platform which generates changes with random bytes, remove all this, and rework testing when proper simulation is possible with test / sim consensus.
///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
fn generate_random_utxos(amount: usize) -> UtxoCollection {
    let mut rng = rand::thread_rng();
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection.insert(generate_random_outpoint(), generate_random_utxo());
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
fn generate_random_utxo() -> UtxoEntry {
    let mut rng = rand::thread_rng();
    UtxoEntry::new(
        rng.gen_range(1..100_000_000_000_000),
        generate_random_script_public_key(),
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
    pub virtual_state: VirtualChangeSetNotification,
    sender: Sender<ConsensusNotification>,
    pub receiver: Receiver<ConsensusNotification>,
}

impl VirtualChangeEmulator {
    pub fn new() -> Self {
        let (s, r) = unbounded::<ConsensusNotification>();
        Self { utxo_collection: UtxoCollection::new(), virtual_state: VirtualChangeSetNotification::default(), sender: s, receiver: r }
    }

    pub fn fill_utxo_collection(&mut self, amount: usize) {
        self.utxo_collection = generate_random_utxos(amount);
    }

    pub fn change_virtual_state(&mut self, remove_amount: usize, add_amount: usize, tip_amount: usize) {
        self.virtual_state
            .virtual_utxo_diff
            .remove
            .extend(self.utxo_collection.iter().map(move |(k, v)| (k.clone(), v.clone())).take(remove_amount));
        println!("here4");
        self.virtual_state.virtual_utxo_diff.add.extend(generate_random_utxos(add_amount));
        println!("{:?}", self.virtual_state.virtual_utxo_diff.add);
        println!("here5");
        self.utxo_collection.extend(self.virtual_state.virtual_utxo_diff.add.iter().map(move |(k, v)| (k.clone(), v.clone())));
        println!("here6");
        self.virtual_state.virtual_parents = generate_new_tips(tip_amount);
        println!("here7");
        self.virtual_state.virtual_selected_parent_blue_score = 0;
        println!("here8");
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
