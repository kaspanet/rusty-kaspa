use kaspa_consensus_core::{
    subnets::SubnetworkId,
    tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
};
use rand::{rngs::SmallRng, Rng};

use crate::testutils::generate::from_rand::{hash::generate_random_hash, utxo::generate_random_p2pk_script_public_key};

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
