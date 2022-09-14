use std::sync::Arc;

use crate::{header::Header, tx::Transaction, BlueWorkType};
use hashes::Hash;

#[derive(Debug, Clone)]
pub struct Block {
    pub header: Header,
    pub transactions: Arc<Vec<Transaction>>,
}

impl Block {
    pub fn new(
        version: u16, parents: Vec<Hash>, timestamp: u64, bits: u32, nonce: u64, daa_score: u64,
        blue_work: BlueWorkType, blue_score: u64,
    ) -> Self {
        Self {
            header: Header::new(
                version,
                parents,
                Default::default(),
                timestamp,
                bits,
                nonce,
                daa_score,
                blue_work,
                blue_score,
            ),
            transactions: Arc::new(Vec::new()),
        }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header, transactions: Arc::new(Vec::new()) }
    }

    pub fn is_header_only(&self) -> bool {
        self.transactions.is_empty()
    }

    pub fn hash(&self) -> Hash {
        self.header.hash
    }
}
