use crate::constants::BLOCK_VERSION;
use consensus_core::{
    block::Block,
    header::Header,
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use hashes::{Hash, HASH_SIZE};
use rand::{rngs::ThreadRng, seq::SliceRandom, Rng};

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header {
        version: BLOCK_VERSION,
        hash,
        parents_by_level: vec![parents],
        hash_merkle_root: Default::default(),
        accepted_id_merkle_root: Default::default(),
        utxo_commitment: Default::default(),
        nonce: 0,
        timestamp: 0,
        daa_score: 0,
        bits: 0,
        blue_work: 0.into(),
        blue_score: 0,
        pruning_point: Default::default(),
    }
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block::from_header(header_from_precomputed_hash(hash, parents))
}

pub fn generate_random_utxos_from_script_public_key_pool(
    rng: &mut ThreadRng,
    amount: usize,
    script_public_key_pool: Vec<ScriptPublicKey>,
) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection.insert(
            generate_random_outpoint(&mut rng.clone()),
            generate_random_utxo_from_script_public_key_pool(&mut rng.clone(), script_public_key_pool.clone()),
        );
        i += 1;
    }
    collection
}

pub fn generate_random_hash(rng: &mut ThreadRng) -> Hash {
    let random_bytes = rng.gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

pub fn generate_random_outpoint(rng: &mut ThreadRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(&mut rng.clone()), rng.gen::<u32>())
}

pub fn generate_random_utxo_from_script_public_key_pool(
    rng: &mut ThreadRng,
    script_public_key_pool: Vec<ScriptPublicKey>,
) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(rng).expect("expected_script_public key").clone(),
        rng.gen(),
        rng.gen_bool(0.5),
    )
}

pub fn generate_random_utxo(rng: &mut ThreadRng) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_script_public_key(&mut rng.clone()),
        rng.gen(),
        rng.gen_bool(0.5),
    )
}

///Note: this generates schnorr p2pk script public keys.
pub fn generate_random_script_public_key(rng: &mut ThreadRng) -> ScriptPublicKey {
    let mut script: ScriptVec = (0..32).map(|_| rng.gen()).collect();
    script.insert(0, 0x20);
    script.push(0xac);
    ScriptPublicKey::new(0_u16, script)
}

pub fn generate_random_hashes(rng: &mut ThreadRng, amount: usize) -> Vec<Hash> {
    let mut tips = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        tips.push(generate_random_hash(&mut rng.clone()));
        i += 1;
    }
    tips
}

///Note: generate_random_block is filled with random data, it does not represent a consensus-valid block!
pub fn generate_random_block(
    rng: &mut ThreadRng,
    parent_amount: usize,
    number_of_transactions: usize,
    input_amount: usize,
    output_amount: usize,
) -> Block {
    Block::new(
        generate_random_header(&mut rng.clone(), parent_amount),
        generate_random_transactions(&mut rng.clone(), number_of_transactions, input_amount, output_amount),
    )
}

///Note: generate_random_header is filled with random data, it does not represent a consensus-valid header!
pub fn generate_random_header(rng: &mut ThreadRng, parent_amount: usize) -> Header {
    Header::new(
        rng.gen(),
        vec![generate_random_hashes(&mut rng.clone(), parent_amount)],
        generate_random_hash(&mut rng.clone()),
        generate_random_hash(&mut rng.clone()),
        generate_random_hash(&mut rng.clone()),
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen::<u64>().into(),
        rng.gen(),
        generate_random_hash(&mut rng.clone()),
    )
}

///Note: generate_random_transaction is filled with random data, it does not represent a consensus-valid transaction!
pub fn generate_random_transaction(rng: &mut ThreadRng, input_amount: usize, output_amount: usize) -> Transaction {
    Transaction::new(
        rng.gen(),
        generate_random_transaction_inputs(&mut rng.clone(), input_amount),
        generate_random_transaction_outputs(&mut rng.clone(), output_amount),
        rng.gen(),
        SubnetworkId::from_byte(rng.gen()),
        rng.gen(),
        (0..20).map(|_| rng.gen()).collect(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transactions!
pub fn generate_random_transactions(
    rng: &mut ThreadRng,
    amount: usize,
    input_amount: usize,
    output_amount: usize,
) -> Vec<Transaction> {
    Vec::from_iter((0..amount).map(move |_| generate_random_transaction(&mut rng.clone(), input_amount, output_amount)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction input!
pub fn generate_random_transaction_input(rng: &mut ThreadRng) -> TransactionInput {
    TransactionInput::new(
        generate_random_transaction_outpoint(&mut rng.clone()),
        (0..32).map(|_| rng.gen()).collect(),
        rng.gen(),
        rng.gen(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_inputs(rng: &mut ThreadRng, amount: usize) -> Vec<TransactionInput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_input(&mut rng.clone())))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_output(rng: &mut ThreadRng) -> TransactionOutput {
    TransactionOutput::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_script_public_key(&mut rng.clone()),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outputs(rng: &mut ThreadRng, amount: usize) -> Vec<TransactionOutput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_output(&mut rng.clone())))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outpoint(rng: &mut ThreadRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(&mut rng.clone()), rng.gen())
}

//TODO: create `assert_eq_<kaspa-sturct>!()` helper macros in `consensus::test_helpers`
