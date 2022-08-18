use crate::constants::BLOCK_VERSION;
use consensus_core::{block::Block, header::Header};
use hashes::Hash;

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header { version: BLOCK_VERSION, hash, parents, nonce: 0, time_in_ms: 0 }
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block { header: header_from_precomputed_hash(hash, parents) }
}
