use crate::header::Header;
use hashes::Hash;

pub struct Block {
    pub header: Header,
}

impl Block {
    pub fn new(version: u16, parents: Vec<Hash>, timestamp: u64, bits: u32, nonce: u64, daa_score: u64) -> Self {
        Self { header: Header::new(version, parents, timestamp, bits, nonce, daa_score) }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header }
    }
}
