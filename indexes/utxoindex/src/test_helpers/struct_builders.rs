use consensus_core::{
    notify::{ConsensusNotification, PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification},
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
    BlockHashSet, HashMapCustomHasher,
};
use hashes::{Hash, HASH_SIZE};
use rand::prelude::SliceRandom;
use rand::Rng;
// TODO: this is an ineffecient, Ad-hoc testing helper / platform which emulates virtual changes with random bytes,
// remove all this, and rework testing when proper simulation is possible with test / sim consensus.
// Note: generated structs are generally filled with random bytes ad do not represent fully consensus conform and valid structs.

pub fn generate_random_utxos(amount: usize, script_public_key_pool: Vec<ScriptPublicKey>) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection.insert(generate_random_outpoint(), generate_random_utxo(script_public_key_pool.clone()));
        i += 1;
    }
    collection
}

pub fn generate_random_hash() -> Hash {
    let random_bytes = rand::thread_rng().gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
pub fn generate_random_outpoint() -> TransactionOutpoint {
    let mut rng = rand::thread_rng();
    TransactionOutpoint::new(generate_random_hash(), rng.gen::<u32>())
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
pub fn generate_random_utxo(script_public_key_pool: Vec<ScriptPublicKey>) -> UtxoEntry {
    let mut rng = rand::thread_rng();
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(&mut rng).expect("expected_script_public key").clone(),
        rng.gen_range(1..100_000_000),
        rng.gen_bool(0.1),
    )
}

///Note: generated structs are generally filled with random bytes do not represent fully valid consensus utxos.
pub fn generate_random_script_public_key() -> ScriptPublicKey {
    let mut script: ScriptVec = (0..32).map(|_| rand::random::<u8>()).collect();
    script.insert(0, 0x20);
    script.push(0xac);
    ScriptPublicKey::new(0_u16, script)
}

///Note: generated structs are generally filled with random bytes ad do not represent fully valid consensus utxos.
pub fn generate_new_tips(amount: usize) -> Vec<Hash> {
    let mut tips = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        tips.push(generate_random_hash());
        i += 1;
    }
    tips
}
