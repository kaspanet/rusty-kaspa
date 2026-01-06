#![no_std]

extern crate alloc;
extern crate core;

// todo set me via env
pub const GENESIS_TX_ID: [u8; 32] = [0; 32];

pub mod tx;
