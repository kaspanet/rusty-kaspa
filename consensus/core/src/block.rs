use crate::{
    coinbase::MinerData,
    header::Header,
    tx::{Transaction, TransactionId},
    BlueWorkType,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

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

/// An abstraction for a recallable transaction selector with persistent state
pub trait TemplateTransactionSelector {
    /// Expected to return a batch of transactions which were not previously selected.
    /// The batch will typically contain sufficient transactions to fill the block
    /// mass (along with the previously unrejected txs), or will drain the selector    
    fn select_transactions(&mut self) -> Vec<Transaction>;

    /// Should be used to report invalid transactions obtained from the *most recent*
    /// `select_transactions` call. Implementors should use this call to internally
    /// track the selection state and discard the rejected tx from internal occupation calculations
    fn reject_selection(&mut self, tx_id: TransactionId);

    /// Determine whether this was an overall successful selection episode
    fn is_successful(&self) -> bool;
}

/// Block template build mode
#[derive(Clone, Copy, Debug)]
pub enum TemplateBuildMode {
    /// Block template build can possibly fail if `TemplateTransactionSelector::is_successful` deems the operation unsuccessful.
    ///
    /// In such a case, the build fails with `BlockRuleError::InvalidTransactionsInNewBlock`.
    Standard,

    /// Block template build always succeeds. The built block contains only the validated transactions.
    Infallible,
}

/// A block template for miners.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    pub block: MutableBlock,
    pub miner_data: MinerData,
    pub coinbase_has_red_reward: bool,
    pub selected_parent_timestamp: u64,
    pub selected_parent_daa_score: u64,
    pub selected_parent_hash: Hash,
}

impl BlockTemplate {
    pub fn new(
        block: MutableBlock,
        miner_data: MinerData,
        coinbase_has_red_reward: bool,
        selected_parent_timestamp: u64,
        selected_parent_daa_score: u64,
        selected_parent_hash: Hash,
    ) -> Self {
        Self { block, miner_data, coinbase_has_red_reward, selected_parent_timestamp, selected_parent_daa_score, selected_parent_hash }
    }

    pub fn to_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.block.header.daa_score, self.block.header.blue_work, self.selected_parent_hash)
    }
}

/// An opaque data structure representing a unique approximate identifier for virtual state. Note that it is
/// approximate in the sense that in rare cases a slightly different virtual state might produce the same identifier,
/// hence it should be used for cache-like heuristics only
#[derive(PartialEq)]
pub struct VirtualStateApproxId {
    daa_score: u64,
    blue_work: BlueWorkType,
    sink: Hash,
}

impl VirtualStateApproxId {
    pub fn new(daa_score: u64, blue_work: BlueWorkType, sink: Hash) -> Self {
        Self { daa_score, blue_work, sink }
    }
}
