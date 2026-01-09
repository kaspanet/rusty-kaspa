#![no_std]

extern crate alloc;
extern crate core;

// todo set me via env
pub const GENESIS_TX_ID: [u8; 32] = [0; 32];

// pub mod tx;

#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PublicInput {
    pub prev_state: u64,
    pub new_state: u64,
    pub payload_diff: u64,
}
