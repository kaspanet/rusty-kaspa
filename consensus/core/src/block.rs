use crate::{header::Header, tx::Transaction};
use hashes::Hash;

#[derive(Debug, Clone)]
pub struct Block {
    pub header: Header,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn new(version: u16, parents: Vec<Hash>, timestamp: u64, bits: u32, nonce: u64, daa_score: u64) -> Self {
        Self { header: Header::new(version, parents, timestamp, bits, nonce, daa_score), transactions: Vec::new() }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header, transactions: Vec::new() }
    }

    pub fn is_header_only(&self) -> bool {
        self.transactions.is_empty()
    }
}
