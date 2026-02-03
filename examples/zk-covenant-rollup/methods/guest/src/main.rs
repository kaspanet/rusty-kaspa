#![no_std]
#![no_main]

use bytemuck::Zeroable;
use risc0_zkvm::{
    guest::env,
    serde::{WordRead, WordWrite},
};
use zk_covenant_rollup_core::{action::{Action, VersionedActionRaw}, is_action_tx_id, payload_digest, seq_commit::{calc_accepted_id_merkle_root, seq_commitment_leaf, StreamingMerkleBuilder}, state::State, tx_id_v1, PublicInput};

risc0_zkvm::guest::entry!(main);

pub fn main() {
    let mut public_input = PublicInput::zeroed();
    let mut stdin = env::stdin();
    stdin.read_words(public_input.as_words_mut()).unwrap();
    let mut state = State::default();
    stdin.read_words(state.as_word_slice_mut()).unwrap();

    // Compute hash of previous state and verify against public input
    let prev_hash = state.hash();
    if prev_hash != public_input.prev_state_hash {
        panic!("Previous state hash mismatch");
    }

    let mut chain_len_to_prove = 0u32;
    stdin.read_words(core::slice::from_mut(&mut chain_len_to_prove)).unwrap();

    let mut prev_seq_commitment = public_input.prev_seq_commitment;
    for _ in 0..chain_len_to_prove {
        let mut tx_id_count = 0u32;
        stdin.read_words(core::slice::from_mut(&mut tx_id_count)).unwrap();

        let mut merkle_builder = StreamingMerkleBuilder::new();

        for _ in 0..tx_id_count {
            let mut version = 0u32;
            stdin.read_words(core::slice::from_mut(&mut version)).unwrap();
            let version = u16::try_from(version).unwrap();

            // Check if this is a transaction that needs processing
            let tx_id = if version == 0 {
                let mut tx_id = [0u32; 8];
                stdin.read_words(&mut tx_id).unwrap();
                tx_id
            } else {
                let mut payload = [0u32; size_of::<VersionedActionRaw>() / 4];
                stdin.read_words(&mut payload).unwrap();
                let mut rest_digest = [0; 8];
                stdin.read_words(&mut rest_digest).unwrap();
                let payload_digest = payload_digest(&payload);
                let tx_id = tx_id_v1(&payload_digest, &rest_digest);

                if is_action_tx_id(&tx_id) {
                    let versioned_action_raw = VersionedActionRaw::from_words(payload);
                    if let Ok(action) = Action::try_from(versioned_action_raw.action_raw) {
                        let output = action.execute();
                        state.add_new_result(action, output);
                    }
                }
                tx_id
            };

            // Compute the leaf digest and add to merkle tree
            let digest = seq_commitment_leaf(&tx_id, version);
            merkle_builder.add_leaf(digest);
        }

        // Finalize the merkle root for this block
        let tx_digests_root = merkle_builder.finalize();

        // Update the sequence commitment chain
        prev_seq_commitment = calc_accepted_id_merkle_root(&prev_seq_commitment, &tx_digests_root);
    }

    let actual_seq_commitment = prev_seq_commitment;

    let new_hash = state.hash();
    env::journal().write_words(&public_input.as_words()).unwrap();
    env::journal().write_words(&new_hash).unwrap();
    env::journal().write_words(&actual_seq_commitment).unwrap();
}
