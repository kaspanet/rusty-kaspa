#![no_std]
#![no_main]

extern crate alloc;

mod auth;
mod block;
mod input;
mod journal;
mod state;
mod tx;
mod witness;

use risc0_zkvm::guest::env;
use zk_covenant_rollup_core::seq_commit::calc_accepted_id_merkle_root;

risc0_zkvm::guest::entry!(main);

pub fn main() {
    let mut stdin = env::stdin();

    // Read and verify public input
    let public_input = input::read_public_input(&mut stdin);
    let mut state_root = public_input.prev_state_hash;

    // Process all blocks
    let chain_len = input::read_u32(&mut stdin);
    let mut seq_commitment = public_input.prev_seq_commitment;

    for _ in 0..chain_len {
        let block_root = block::process_block(&mut stdin, &mut state_root);
        seq_commitment = calc_accepted_id_merkle_root(&seq_commitment, &block_root);
    }

    // Write journal output
    journal::write_output(&public_input, &state_root, &seq_commitment);
}
