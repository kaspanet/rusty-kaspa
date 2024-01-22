use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
};

use kaspa_consensus_core::{
    tx::{TransactionId, TransactionIndexType},
    BlockHashMap, BlockHashSet,
};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

pub type TxHashSet = HashSet<TransactionId>;
pub type TxOffsetById = HashMap<TransactionId, TxOffset>;
pub type BlockAcceptanceOffsetByHash = BlockHashMap<BlockAcceptanceOffset>;
pub type MergesetIndexType = u16;

/// A struct holding tx diffs to be committed to the txindex via `added` and `removed`.
#[derive(Debug, Clone, Default)]
pub struct TxOffsetDiff {
    pub added: TxOffsetById,
    pub removed: TxHashSet,
}

impl TxOffsetDiff {
    pub fn new(added: TxOffsetById, removed: TxHashSet) -> Self {
        Self { added, removed }
    }
}

/// A struct holding block accepted diffs to be committed to the txindex via `added` and `removed`.
#[derive(Debug, Clone, Default)]
pub struct BlockAcceptanceOffsetDiff {
    pub added: BlockAcceptanceOffsetByHash,
    pub removed: BlockHashSet,
}

impl BlockAcceptanceOffsetDiff {
    pub fn new(added: BlockAcceptanceOffsetByHash, removed: BlockHashSet) -> Self {
        Self { added, removed }
    }
}

/// Holds a [`Transaction`]'s inlcluding_block [`Hash`] and [`TransactionIndexType`], for reference to the [`Transaction`] of a [`DbBlockTransactionsStore`].
#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct TxOffset {
    pub including_block: Hash,
    pub transaction_index: TransactionIndexType,
}

impl TxOffset {
    pub fn new(including_block: Hash, transaction_index: TransactionIndexType) -> Self {
        Self { including_block, transaction_index }
    }
}

impl MemSizeEstimator for TxOffset {
    fn estimate_mem_units(&self) -> usize {
        1
    }

    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

/// Holds a Block's accepting [`Hash`] and [`MergeSetIDX`] of a block, for reference to the block's [`MergesetBlockAcceptanceData`] of a [`DbAcceptanceDataStore`].
#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct BlockAcceptanceOffset {
    pub accepting_block: Hash,
    pub mergeset_index: MergesetIndexType,
}

impl MemSizeEstimator for BlockAcceptanceOffset {
    fn estimate_mem_units(&self) -> usize {
        1
    }

    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

impl BlockAcceptanceOffset {
    pub fn new(accepting_block: Hash, mergeset_index: MergesetIndexType) -> Self {
        Self { accepting_block, mergeset_index }
    }
}
