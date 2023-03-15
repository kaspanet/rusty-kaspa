use std::sync::Arc;

use crate::{coinbase::MinerData, header::Header, tx::Transaction};
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

    pub fn from_arcs(header: Arc<Header>, transactions: Arc<Vec<Transaction>>) -> Self {
        Self { header, transactions }
    }

    pub fn from_header_arc(header: Arc<Header>) -> Self {
        Self { header, transactions: Arc::new(Vec::new()) }
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

    /// WARNING: To be used for test purposes only
    pub fn from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
        Block::from_header(Header::from_precomputed_hash(hash, parents))
    }
}

/// A block template for miners.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    pub block: MutableBlock,
    pub miner_data: MinerData,
    pub coinbase_has_red_reward: bool,
    pub selected_parent_timestamp: u64,
}

impl BlockTemplate {
    pub fn new(block: MutableBlock, miner_data: MinerData, coinbase_has_red_reward: bool, selected_parent_timestamp: u64) -> Self {
        Self { block, miner_data, coinbase_has_red_reward, selected_parent_timestamp }
    }
}
