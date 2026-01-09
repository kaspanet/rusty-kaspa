#![no_std]
#![no_main]

risc0_zkvm::guest::entry!(main);

use risc0_zkvm::guest::env;
use bytemuck::Zeroable;
use zk_covenant_inline_core::PublicInput;

// use zk_covenant_inline_core::{GENESIS_TX_ID};

pub fn main() {
    let mut public_input: [PublicInput; 1] = [PublicInput::zeroed()];
    env::read_slice(&mut public_input);
    env::commit_slice(&public_input);
    let public_input = public_input[0];

    assert_eq!(public_input.new_state, public_input.prev_state.saturating_add(public_input.payload_diff));
}