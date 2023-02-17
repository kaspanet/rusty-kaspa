//!
//! Model structures which are related to IBD pruning point syncing logic. These structures encode
//! a specific syncing protocol and thus do not belong within consensus core.
//!

use consensus_core::{
    block::Block,
    trusted::{TrustedBlock, TrustedHash, TrustedHeader},
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
};

use crate::common::FlowError;

/// A package of *semi-trusted data* used by a syncing node in order to build
/// the sub-DAG in the anticone and in the recent past of the synced pruning point
pub struct TrustedDataPackage {
    pub daa_window: Vec<TrustedHeader>,
    pub ghostdag_window: Vec<TrustedHash>,
}

impl TrustedDataPackage {
    pub fn new(daa_window: Vec<TrustedHeader>, ghostdag_window: Vec<TrustedHash>) -> Self {
        Self { daa_window, ghostdag_window }
    }

    /// Returns the trusted set -- a sub-DAG in the anti-future of the pruning point which contains
    /// all the blocks and ghostdag data needed in order to validate the headers in the future of
    /// the pruning point
    pub fn build_trusted_subdag(self, entries: Vec<TrustedDataEntry>) -> Result<Vec<TrustedBlock>, FlowError> {
        let mut blocks = Vec::with_capacity(entries.len());
        let mut set = BlockHashSet::new();
        let mut map = BlockHashMap::new();

        for th in self.ghostdag_window.iter() {
            map.insert(th.hash, th.ghostdag.clone());
        }

        for th in self.daa_window.iter() {
            map.insert(th.header.hash, th.ghostdag.clone());
        }

        for entry in entries {
            let block = entry.block;
            if set.insert(block.hash()) {
                if let Some(ghostdag) = map.get(&block.hash()) {
                    blocks.push(TrustedBlock::new(block, ghostdag.clone()));
                } else {
                    return Err(FlowError::ProtocolError("missing ghostdag data for some trusted entries"));
                }
            }
        }

        for th in self.daa_window.iter() {
            if set.insert(th.header.hash) {
                blocks.push(TrustedBlock::new(Block::from_header_arc(th.header.clone()), th.ghostdag.clone()));
            }
        }

        // Topological sort
        blocks.sort_by(|a, b| a.block.header.blue_work.cmp(&b.block.header.blue_work));

        Ok(blocks)
    }
}

/// A block with DAA/Ghostdag indices corresponding to data location within a `TrustedDataPackage`
pub struct TrustedDataEntry {
    pub block: Block,
    pub daa_window_indices: Vec<u64>,
    pub ghostdag_window_indices: Vec<u64>,
    //
    // Rust rewrite note: the indices fields are no longer needed with the way the pruning point anti-future
    // is marinated now. Meaning we simply build this sub-DAG in a way that the usual traversal operations will
    // return the correct blocks/data without the need for explicitly provided indices.
    //
}

impl TrustedDataEntry {
    pub fn new(block: Block, daa_window_indices: Vec<u64>, ghostdag_window_indices: Vec<u64>) -> Self {
        Self { block, daa_window_indices, ghostdag_window_indices }
    }
}
