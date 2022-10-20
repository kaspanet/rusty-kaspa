use std::sync::Arc;

use crate::{header::Header, tx::Transaction};
use hashes::Hash;

/// A mutable block structure where header and transactions within can still be mutated.
#[derive(Debug, Clone)]
pub struct MutableBlock {
    pub header: Header,
    pub transactions: Vec<Transaction>,
}

impl MutableBlock {
    pub fn new(header: Header, txs: Vec<Transaction>) -> Self {
        Self { header, transactions: txs }
    }

    pub fn from_header(header: Header) -> Self {
        Self::new(header, vec![])
    }

    pub fn to_immutable(self) -> Block {
        Block::new(self.header, self.transactions)
    }
}

/// A block structure where the inner header and transactions are wrapped by Arcs for
/// cheap cloning and for cross-thread safety and immutability. Note: no need to wrap
/// this struct with an additional Arc.
#[derive(Debug, Clone)]
pub struct Block {
    pub header: Arc<Header>,
    pub transactions: Arc<Vec<Transaction>>,
}

impl Block {
    pub fn new(header: Header, txs: Vec<Transaction>) -> Self {
        Self { header: Arc::new(header), transactions: Arc::new(txs) }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header: Arc::new(header), transactions: Arc::new(Vec::new()) }
    }

    pub fn is_header_only(&self) -> bool {
        self.transactions.is_empty()
    }

    pub fn hash(&self) -> Hash {
        self.header.hash
    }
}
