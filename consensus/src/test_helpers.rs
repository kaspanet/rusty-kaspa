use kaspa_consensus_core::{
    block::Block,
    header::Header,
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::{Hash, HASH_SIZE};
use rand::{rngs::SmallRng, seq::SliceRandom, Rng};

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header::from_precomputed_hash(hash, parents)
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block::from_precomputed_hash(hash, parents)
}

pub fn generate_random_utxos_from_script_public_key_pool(
    rng: &mut SmallRng,
    amount: usize,
    script_public_key_pool: &[ScriptPublicKey],
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

pub fn generate_random_hash(rng: &mut SmallRng) -> Hash {
    let random_bytes = rng.gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

pub fn generate_random_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(rng), rng.gen::<u32>())
}

pub fn generate_random_utxo_from_script_public_key_pool(rng: &mut SmallRng, script_public_key_pool: &[ScriptPublicKey]) -> UtxoEntry {
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

pub fn generate_random_hashes(rng: &mut SmallRng, amount: usize) -> Vec<Hash> {
    let mut hashes = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        hashes.push(generate_random_hash(rng));
        i += 1;
    }
    hashes
}

///Note: generate_random_block is filled with random data, it does not represent a consensus-valid block!
pub fn generate_random_block(
    rng: &mut SmallRng,
    parent_amount: usize,
    number_of_transactions: usize,
    input_amount: usize,
    output_amount: usize,
) -> Block {
    Block::new(
        generate_random_header(rng, parent_amount),
        generate_random_transactions(rng, number_of_transactions, input_amount, output_amount),
    )
}

///Note: generate_random_header is filled with random data, it does not represent a consensus-valid header!
pub fn generate_random_header(rng: &mut SmallRng, parent_amount: usize) -> Header {
    Header::new_finalized(
        rng.gen(),
        vec![generate_random_hashes(rng, parent_amount)],
        generate_random_hash(rng),
        generate_random_hash(rng),
        generate_random_hash(rng),
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen::<u64>().into(),
        rng.gen(),
        generate_random_hash(rng),
    )
}

///Note: generate_random_transaction is filled with random data, it does not represent a consensus-valid transaction!
pub fn generate_random_transaction(rng: &mut SmallRng, input_amount: usize, output_amount: usize) -> Transaction {
    Transaction::new(
        rng.gen(),
        generate_random_transaction_inputs(rng, input_amount),
        generate_random_transaction_outputs(rng, output_amount),
        rng.gen(),
        SubnetworkId::from_byte(rng.gen()),
        rng.gen(),
        (0..20).map(|_| rng.gen::<u8>()).collect(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transactions!
pub fn generate_random_transactions(rng: &mut SmallRng, amount: usize, input_amount: usize, output_amount: usize) -> Vec<Transaction> {
    Vec::from_iter((0..amount).map(move |_| generate_random_transaction(rng, input_amount, output_amount)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction input!
pub fn generate_random_transaction_input(rng: &mut SmallRng) -> TransactionInput {
    TransactionInput::new(generate_random_transaction_outpoint(rng), (0..32).map(|_| rng.gen::<u8>()).collect(), rng.gen(), rng.gen())
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_inputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionInput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_input(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_output(rng: &mut SmallRng) -> TransactionOutput {
    TransactionOutput::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionOutput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_output(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(rng), rng.gen())
}

//TODO: create `assert_eq_<kaspa-sturct>!()` helper macros in `consensus::test_helpers`
