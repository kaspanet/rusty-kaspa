use std::{
    collections::{HashMap, HashSet},
    mem::size_of, ops::Sub,
};

use kaspa_consensus_core::{
    tx::{TransactionId, TransactionIndexType},
};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

pub type TxHashSet = HashSet<TransactionId>;
pub type TxIndexTxEntryById = HashMap<TransactionId, TxIndexTxEntry>;
pub type AcceptanceDataIndexType = u16;


#[derive(Debug, Clone, Default)]
pub struct TxIndexTxEntryDiff {
    pub added: TxIndexTxEntryById,
    pub removed: TxHashSet,
}

impl TxIndexTxEntryDiff {
    pub fn new(added: TxIndexTxEntryById, removed: TxHashSet) -> Self {
        Self { added, removed }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct TxIndexTxEntry {
    pub inclusion_offset: TxIndexTxInclusionOffset,
    pub accepting_blue_score: u64,
}

impl TxIndexTxEntry {

    #[inline]
    pub fn new(inclusion_offset: TxInclusionOffset, accepting_blue_score: u64) -> Self {
        Self { inclusion_offset, accepting_blue_score }
    }

    #[inline]
    pub fn inclusion_offset(&self) -> TxInclusionOffset {
        self.inclusion_offset
    }

    #[inline]
    pub fn accepting_blue_score(&self) -> u64 {
        self.accepting_blue_score
    }
}

impl MemSizeEstimator for TxIndexEntry {
    #[inline]
    fn estimate_mem_units(&self) -> usize {
        1
    }

    #[inline]
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

/// Holds a [`Transaction`]'s inlcluding_block [`Hash`] and [`TransactionIndexType`], for reference to the [`Transaction`] of a [`DbBlockTransactionsStore`].
#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct TxIndexTxInclusionOffset {
    pub including_block: Hash,
    pub transaction_index: TransactionIndexType,
}

impl TxIndexTxInclusionOffset {

    #[inline]
    pub fn new(including_block: Hash, transaction_index: TransactionIndexType) -> Self {
        Self { including_block, transaction_index }
    }

    #[inline]
    pub fn including_block(&self) -> Hash {
        self.including_block
    }

    #[inline]
    pub fn transaction_index(&self) -> TransactionIndexType {
        self.transaction_index
    }
}

impl MemSizeEstimator for TxInclusionOffset {
    #[inline]
    fn estimate_mem_units(&self) -> usize {
        1
    }

    #[inline]
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct TxIndexSinkData {
    pub hash: Hash,
    pub blue_score: u64,
}

impl TxIndexSinkData {
    #[inline]
    pub fn new(hash: Hash, blue_score: u64) -> Self {
        Self { hash, blue_score }
    }
}


#[derive(Clone, Copy, Deserialize, Serialize, Debug, Hash)]
pub struct TxIndexPruningState {
    pub pruning_point: Hash,
    pub pruning_point_blue_score: u64,
    // The last transaction that was pruned, if pruning is active, else None. 
    // this is used to avoid re-iteration of the db during an interrupted pruning process.
    pub last_scanned_transaction: Option<Hash>,
    pub last_scan_count: u64,
}

impl TxIndexPruningState {
    #[inline]
    pub fn new(pruning_point: Hash, pruning_point_blue_score: u64, last_pruned_transaction: Option<Hash>, last_prune_count: u64) -> Self {
        Self { pruning_point, pruning_point_blue_score, last_pruned_transaction, last_prune_count }
    }

    #[inline]
    pub fn pruning_point(&self) -> Hash {
        self.pruning_point
    }

    #[inline]
    pub fn pruning_point_blue_score(&self) -> u64 {
        self.pruning_point_blue_score
    }

    #[inline]
    pub fn last_pruned_transaction(&self) -> Option<Hash> {
        self.last_pruned_transaction
    }

    #[inline]
    pub fn last_prune_count(&self) -> u64 {
        self.last_prune_count
    }

    #[inline]
    pub fn set_last_pruned_transaction(&mut self, last_pruned_transaction: Hash) {
        self.last_pruned_transaction = Some(last_pruned_transaction);
    }

    #[inline]
    pub fn reset_last_pruned_transaction(&mut self) {
        self.last_pruned_transaction = None;
    }

    #[inline]
    pub fn add_to_last_prune_count(&mut self, count_to_add: u64) {
        self.last_prune_count += count_to_add;
    }

    #[inline]
    pub fn reset_last_prune_count(&mut self) {
        self.last_prune_count = 0;
    }
}

#[cfg(test)]
pub mod test {
    use kaspa_consensus_core::{config::{params::Params, Config}, network::NetworkType};

    use crate::models::txindex::AcceptanceDataIndexType;

    #[test]
    fn test_block_mergest_index_type_max() {
        NetworkType::iter().for_each(|network_type| {
            Config::from(network_type);
            assert!(Params::from(network_type).mergeset_size_limit <= AcceptanceDataIndexType::MAX as u64);
        });
    }
}

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
pub type AcceptanceDataIndexType = u16;

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
    pub acceptance_data_index: AcceptanceDataIndexType,
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
    pub fn new(accepting_block: Hash, acceptance_data_index: AcceptanceDataIndexType) -> Self {
        Self { accepting_block, acceptance_data_index }
    }
}

#[cfg(test)]
pub mod test {
    use kaspa_consensus_core::{config::params::Params, network::NetworkType};

    use crate::models::txindex::AcceptanceDataIndexType;

    #[test]
    fn test_block_mergest_index_type_max() {
        NetworkType::iter().for_each(|network_type| {
            assert!(Params::from(network_type).mergeset_size_limit <= AcceptanceDataIndexType::MAX as u64);
        });
    }
}
