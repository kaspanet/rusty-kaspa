use crate::testutils::generate::from_rand::hash::generate_random_hash;
use kaspa_consensus_core::{
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use rand::{rngs::SmallRng, seq::SliceRandom, Rng};

pub fn generate_random_utxos_from_script_public_key_pool(
    rng: &mut SmallRng,
    amount: usize,
    script_public_key_pool: &Vec<ScriptPublicKey>,
) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection
            .insert(generate_random_outpoint(rng), generate_random_utxo_from_script_public_key_pool(rng, script_public_key_pool));
        i += 1;
    }
    collection
}

pub fn generate_random_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(rng), rng.gen::<u32>())
}

pub fn generate_random_utxo_from_script_public_key_pool(
    rng: &mut SmallRng,
    script_public_key_pool: &Vec<ScriptPublicKey>,
) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(rng).expect("expected_script_public key").clone(),
        rng.gen(),
        rng.gen_bool(0.5),
    )
}

pub fn generate_random_utxo(rng: &mut SmallRng) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
        rng.gen(),
        rng.gen_bool(0.5),
    )
}

///Note: this generates schnorr p2pk script public keys.
pub fn generate_random_p2pk_script_public_key(rng: &mut SmallRng) -> ScriptPublicKey {
    let mut script: ScriptVec = (0..32).map(|_| rng.gen()).collect();
    script.insert(0, 0x20);
    script.push(0xac);
    ScriptPublicKey::new(0_u16, script)
}
