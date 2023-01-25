use consensus_core::{utxo::utxo_collection::UtxoCollection, tx::{ScriptPublicKey, UtxoEntry, ScriptVec, TransactionOutpoint}, notify::{VirtualChangeSetNotification, ConsensusNotification}};
use rand::Rng;
use hashes::{Hash, HASH_SIZE};
use async_std::channel::{unbounded, Receiver, Sender};



//TODO: this is an Ad hoc testing helper / platform which generates changes with random bytes, remove all this, and rework testing when proper simulation is possible with test / sim consensus.
fn generate_random_utxos(amount:usize) -> UtxoCollection {
    let mut rng = rand::thread_rng();
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection.insert(
            generate_random_outpoint(),
            generate_random_utxo()
        );
            i+=1;
    }
    collection
}

fn generate_random_hash() -> Hash {
    let mut bytes: [u8; HASH_SIZE];
    rng.fill(bytes);
    Hash::from_bytes(bytes)
}

fn generate_random_outpoint() -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(), rng.gen::<u32>())
}

fn generate_random_utxo() -> UtxoEntry {
    let mut rng = rand::thread_rng();
    UtxoEntry::new(
        rng.gen_range(1..100_000_000_000_000),
        generate_random_script_public_key(), 
        rng.gen_range(1..100_000_000),
        rng.gen_bool(0.1)
    )
}

fn generate_random_script_public_key() -> ScriptPublicKey {
    let mut rng = rand::thread_rng();
    let &mut script = ScriptVec::new();
    rng.fill( script);
    ScriptPublicKey::new(0, script)
}

fn generate_new_tips(amount: usize) -> Vec<Hash> {
    let tips = Vec::with_capacity::<Hash>(amount);
    let mut i = 0;
    while i < amount {
        tips.push(generate_random_hash());
        i+=1;
    }
    tips
}

pub struct VirtualChangeEmulator {
    utxo_collection: UtxoCollection,
    virtual_state: VirtualChangeSetNotification,
    sender: Sender<ConsensusNotification>,
    pub receiver: Receiver<ConsensusNotification>
}

impl VirtualChangeEmulator {
    pub fn new() -> Self {
        let (s, r) = unbounded::<ConsensusNotification>();
        Self { 
            utxo_collection: UtxoCollection::new(), 
            virtual_state: VirtualChangeSetNotification::default(),
            sender: s,
            receiver: r,
        }
    }

    pub fn fill_utxo_collection(&self, amount:usize) -> Self {
        self.utxo_collection = generate_random_utxos(amount)
    }

    pub fn change_virtual_state(&self, remove_amount:usize, add_amount:usize, tip_amount:usize) -> Self {
        self.virtual_state.virtual_utxo_diff.remove.extend(self.utxo_collection.iter().take(remove_amount));
        self.virtual_state.virtual_utxo_diff.added.extend(generate_random_utxos(add_amount));
        self.utxo_collection.extend(self.virtual_state.virtual_utxo_diff.added);
        self.virtual_state.virtual_parents = generate_new_tips(tips);
        self.virtual_state.virtual_selected_parent_blue_score = 0;
        self.virtual_state.virtual_daa_score = 0;
    }

    pub fn signal_virtual_state(&self) {
        self.sender.try_send(ConsensusNotification::VirtualStateNotification(self.virtual_state)).expect("expected send")
    }

    pub fn signal_utxoset_override(&self) {
        self.sender.try_send(ConsensusNotification::PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification))
    }
    }
