#![no_std]

extern crate alloc;
extern crate core;

pub mod action;
pub mod seq_commit;
pub mod state;

#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, align(4))]
pub struct PublicInput {
    pub prev_state_hash: [u32; 8],

    pub prev_seq_commitment: [u32; 8],
}

impl PublicInput {
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    pub fn as_words_mut(&mut self) -> &mut [u32] {
        bytemuck::cast_slice_mut(bytemuck::bytes_of_mut(self))
    }
}

pub const ACTION_TX_ID_PREFIX: u32 = u32::from_le_bytes(*b"ACTN");

pub fn payload_digest(payload: &[u32]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"PayloadDigest";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(blake3::keyed_hash(&KEY, bytemuck::cast_slice(payload)).as_bytes());
    out
}

pub fn tx_id_v1(payload_digest: &[u32; 8], rest_digest: &[u32; 8]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"TransactionV1Id";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut hasher = blake3::Hasher::new_keyed(&KEY);
    hasher.update(bytemuck::cast_slice(payload_digest));
    hasher.update(bytemuck::cast_slice(rest_digest));
    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hasher.finalize().as_bytes());
    out
}

const fn domain_to_key(domain: &[u8]) -> [u8; blake3::KEY_LEN] {
    let mut key = [0u8; blake3::KEY_LEN];
    let mut i = 0usize;
    while i < domain.len() {
        key[i] = domain[i];
        i += 1;
    }
    key
}
