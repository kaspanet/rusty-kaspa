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
use zk_covenant_rollup_core::{
    MAX_DELEGATE_INPUTS, build_permission_redeem_bytes, bytes_to_words, p2sh::blake2b_script_hash, pad_to_depth,
    permission_tree::StreamingPermTreeBuilder, required_depth, seq_commit::calc_accepted_id_merkle_root,
};

risc0_zkvm::guest::entry!(main);

// ANCHOR: guest_main
pub fn main() {
    let mut stdin = env::stdin();

    // Read and verify public input
    let public_input = input::read_public_input(&mut stdin);
    let mut state_root = public_input.prev_state_hash;

    // Process all blocks
    let chain_len = input::read_u32(&mut stdin);
    let mut seq_commitment = public_input.prev_seq_commitment;
    let mut perm_builder = StreamingPermTreeBuilder::new();

    for _ in 0..chain_len {
        let block_root = block::process_block(&mut stdin, &mut state_root, &public_input.covenant_id, &mut perm_builder);
        seq_commitment = calc_accepted_id_merkle_root(&seq_commitment, &block_root);
    }

    // Build permission output if exits occurred
    let perm_count = perm_builder.leaf_count();
    let permission_spk_hash = if perm_count > 0 {
        // Read expected redeem script length from host (private input)
        let perm_redeem_script_len = input::read_u32(&mut stdin) as i64;

        let depth = required_depth(perm_count as usize);
        let perm_root = pad_to_depth(perm_builder.finalize(), perm_count, depth);

        // Build once with host-provided length, then assert
        let perm_redeem =
            build_permission_redeem_bytes(&perm_root, perm_count as u64, depth, perm_redeem_script_len, MAX_DELEGATE_INPUTS);
        assert_eq!(perm_redeem.len() as i64, perm_redeem_script_len, "permission redeem script length mismatch");

        // blake2b hash → script_hash
        let script_hash = blake2b_script_hash(&perm_redeem);
        Some(bytes_to_words(script_hash))
    } else {
        None
    };

    // Write journal output
    journal::write_output(&public_input, &state_root, &seq_commitment, permission_spk_hash.as_ref());
}
// ANCHOR_END: guest_main
