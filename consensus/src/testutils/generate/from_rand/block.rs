use kaspa_consensus_core::{block::Block, header::Header};
use rand::{rngs::SmallRng, Rng};

use crate::testutils::generate::from_rand::{
    hash::{generate_random_hash, generate_random_hashes},
    tx::generate_random_transactions,
};

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
