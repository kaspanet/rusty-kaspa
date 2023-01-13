//TODO

use std::collections::HashMap;

use consensus::model::stores::{virtual_state::VirtualState, DB};
use consensus_core::{utxo::{utxo_diff::UtxoDiff, utxo_collection::UtxoCollection}, tx::{UtxoEntry, ScriptPublicKeys, ScriptPublicKey, ScriptVec, SCRIPT_VECTOR_SIZE, TransactionOutpoint}};
use hashes::{Hash, HASH_SIZE};
use rand::{Fill, Rng, Rng::ThreadRng};
use tokio::sync::mpsc::Sender;
use utxoindex::utxoindex::UtxoIndex;

struct TestVirtualChangeEmulator {
    tips: Vec<Hash>,
    utxo_collection: UtxoCollection,
    script_public_keys: ScriptPublicKeys,
    rng: ThreadRng,
    test_send: Sender<VirtualState>,
    daa_score: u64,
    virtual_state: VirtualState,
}

impl TestVirtualChangeEmulator {

    fn new() -> Self {
        Self {
            tips: Vec::<Hash>::new(),
            utxo_collection: HashMap::new::<TransactionOutpoint, UtxoEntry>(),
            rng: rand::thread_rng(),
            script_public_keys: todo!(),
            test_send: todo!(),
            daa_score: 1,
            virtual_state: todo!(),
        }
    }

    fn generate_random_utxo(mut self, coinbase_probability: f64) -> UtxoEntry {
        UtxoEntry::new(
            rand::thread_rng().gen_range(0..100_000_000_000), 
            self.generate_random_script_public_key(), 
            self.daa_score, 
            self.rng.gen_bool(0.1))
    }

    fn generate_random_script_public_key(&self) -> ScriptPublicKey {
        let mut bytes = [u8; SCRIPT_VECTOR_SIZE];
        self.rng.fill_bytes(&mut bytes);
        ScriptPublicKey::new
        (
            1, 
            ScriptVec::from_buf(bytes)
        )
    }

    fn generate_random_parents(&self) -> Vec<Hash> {
        let mut i = 0;
        let mut rand_gen = rand::thread_rng();
        let mut parents = Vec::new();
        while i < rand_gen.gen_range(1..32) {
            parents.push(generate_random_hash());
        }
        parents
    }

    fn generate_random_hash() -> Hash {
        let mut bytes = [u8; HASH_SIZE];
        rand::thread_rng().fill_bytes(&mut bytes);
        Hash::from_bytes(bytes)
    }
}