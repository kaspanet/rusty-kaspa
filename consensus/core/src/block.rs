use crate::header::Header;
use hashes::Hash;

pub struct Block {
    pub header: Header,
}

impl Block {
    pub fn new(version: u16, parents: Vec<Hash>, nonce: u64, time_in_ms: u64) -> Self {
        Self { header: Header::new(version, parents, nonce, time_in_ms) }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header }
    }

    /// Temp function for injecting the hash externally
    pub fn from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { header: Header::from_precomputed_hash(hash, parents) }
    }
}
