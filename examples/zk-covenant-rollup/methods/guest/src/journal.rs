use risc0_zkvm::{guest::env, serde::WordWrite};
use zk_covenant_rollup_core::PublicInput;

// ANCHOR: write_output
/// Write the proof output to the journal.
///
/// Journal layout:
///   Base (160 bytes = 40 words):
///     prev_state_hash(32) | prev_seq_commitment(32) | new_state(32) | new_seq(32) | covenant_id(32)
///   With permission (192 bytes = 48 words):
///     ... base ... | permission_spk_hash(32)
#[inline]
pub fn write_output(
    public_input: &PublicInput,
    final_state_root: &[u32; 8],
    final_seq_commitment: &[u32; 8],
    permission_spk_hash: Option<&[u32; 8]>,
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

    // Write covenant_id
    journal.write_words(&public_input.covenant_id).unwrap();

    // Write permission SPK hash when exits are present
    if let Some(hash_words) = permission_spk_hash {
        journal.write_words(hash_words).unwrap();
    }
}
// ANCHOR_END: write_output
