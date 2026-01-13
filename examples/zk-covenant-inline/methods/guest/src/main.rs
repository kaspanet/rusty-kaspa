#![no_std]
#![no_main]

risc0_zkvm::guest::entry!(main);

use risc0_zkvm::guest::env;
use bytemuck::Zeroable;
use zk_covenant_inline_core::{Action, PublicInput, State};
//

pub fn main() {
    let mut public_input = PublicInput::zeroed();
    env::read_slice(core::slice::from_mut(&mut public_input));
    assert_eq!(public_input.versioned_action_raw.action_version, 0);
    env::commit_slice(core::slice::from_ref(&public_input));
    let mut state = State::default();
    env::read_slice(core::slice::from_mut(&mut state));

    // Compute hash of previous state and verify against public input
    let prev_hash = state.hash();
    if prev_hash != public_input.prev_state_hash {
        panic!("Previous state hash mismatch");
    }

    let action: Action = public_input.versioned_action_raw.action_raw.try_into().unwrap_or_else(|_| panic!("Invalid action"));

    let output = match action {
        Action::Fib(n) => fib(n),
        Action::Factorial(n) => factorial(n),
    };

    state.add_new_result(action, output);
    let new_hash = state.hash();
    env::commit_slice(&new_hash);
}

fn fib(n: u8) -> u32 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    let mut a = 0u32;
    let mut b = 1u32;
    for _ in 2..=n {
        let temp = a + b;
        a = b;
        b = temp;
    }
    b
}

fn factorial(n: u8) -> u32 {
    let mut res = 1u32;
    for i in 1..=n {
        res = res.saturating_mul(i as u32); // Use saturating_mul to handle overflow gracefully
    }
    res
}