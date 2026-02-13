use risc0_zkvm::{guest::env, serde::WordWrite};
use zk_covenant_rollup_core::PublicInput;

/// Write the proof output to the journal.
///
/// Journal layout (208 bytes = 52 words):
///   prev_state_hash      (32B)  — from public input
///   prev_seq_commitment  (32B)  — from public input
///   new_state_hash       (32B)
///   new_seq_commitment   (32B)
///   exit_amount          (8B)   — total withdrawal amount (0 if no exits)
///   exit_root            (32B)  — permission tree root (empty if no exits)
///   exit_unclaimed_count (8B)   — number of permission leaves (0 if no exits)
///   covenant_id          (32B)  — from public input
#[inline]
pub fn write_output(
    public_input: &PublicInput,
    final_state_root: &[u32; 8],
    final_seq_commitment: &[u32; 8],
    exit_amount: u64,
    exit_root: &[u32; 8],
    exit_unclaimed_count: u64,
) {
    let mut journal = env::journal();

    // Write prev_state_hash and prev_seq_commitment individually
    // (covenant_id is written at the end, not adjacent to them)
    journal.write_words(&public_input.prev_state_hash).unwrap();
    journal.write_words(&public_input.prev_seq_commitment).unwrap();

    // Write new state root
    journal.write_words(final_state_root).unwrap();

    // Write final sequence commitment
    journal.write_words(final_seq_commitment).unwrap();

    // Write exit fields
    let exit_amount_words: [u32; 2] = bytemuck::cast(exit_amount);
    journal.write_words(&exit_amount_words).unwrap();

    journal.write_words(exit_root).unwrap();

    let exit_count_words: [u32; 2] = bytemuck::cast(exit_unclaimed_count);
    journal.write_words(&exit_count_words).unwrap();

    // Write covenant_id at end
    journal.write_words(&public_input.covenant_id).unwrap();
}
