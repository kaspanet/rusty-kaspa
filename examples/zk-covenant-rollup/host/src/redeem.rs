use kaspa_txscript::{
    opcodes::codes::{OpTrue, OpVerify, OpZkPrecompile},
    script_builder::ScriptBuilder,
    zk_precompiles::tags::ZkTag,
};

use crate::covenant::RollupCovenant;
use crate::risc0_g16::Risc0Groth16Verify;

/// Build the rollup redeem script with embedded state
pub fn build_redeem_script(
    prev_state_hash: [u32; 8],
    prev_seq_commitment: [u32; 8],
    redeem_script_len: i64,
    program_id: &[u8],
    zk_tag: &ZkTag,
) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // 66-byte prefix: OpData32 || prev_seq_commitment || OpData32 || prev_state_hash
    builder.add_data(bytemuck::bytes_of(&prev_seq_commitment)).unwrap();
    builder.add_data(bytemuck::bytes_of(&prev_state_hash)).unwrap();

    // Stash prev values to alt stack
    builder.stash_prev_values().unwrap();

    // Get new_seq_commitment from block_prove_to
    builder.obtain_new_seq_commitment().unwrap();

    // Build new redeem prefix and stash new values
    builder.build_next_redeem_prefix_rollup().unwrap();

    // Extract suffix and concat
    builder.extract_redeem_suffix_and_concat(redeem_script_len).unwrap();

    // Hash redeem → SPK, verify output
    builder.hash_redeem_to_spk().unwrap();
    builder.verify_output_spk().unwrap();

    // Build journal preimage and hash
    builder.build_and_hash_journal().unwrap();

    // Push program_id
    builder.add_data(program_id).unwrap();

    // ZK verify based on proof type
    match *zk_tag {
        ZkTag::R0Succinct => {
            builder.add_data(&[ZkTag::R0Succinct as u8]).unwrap();
            builder.add_op(OpZkPrecompile).unwrap();
            builder.add_op(OpVerify).unwrap();
        }
        ZkTag::Groth16 => {
            builder.verify_risc0_groth16().unwrap();
        }
    }

    // Guards
    builder.verify_input_index_zero().unwrap();
    builder.verify_covenant_single_output().unwrap();
    builder.add_op(OpTrue).unwrap();

    builder.drain()
}
