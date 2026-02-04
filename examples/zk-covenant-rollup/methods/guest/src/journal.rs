use risc0_zkvm::{guest::env, serde::WordWrite};
use zk_covenant_rollup_core::PublicInput;

/// Write the proof output to the journal
pub fn write_output(public_input: &PublicInput, final_state_root: &[u32; 8], final_seq_commitment: &[u32; 8]) {
    let mut journal = env::journal();

    // Write original public input
    journal.write_words(public_input.as_words()).unwrap();

    // Write new state root
    journal.write_words(final_state_root).unwrap();

    // Write final sequence commitment
    journal.write_words(final_seq_commitment).unwrap();
}
