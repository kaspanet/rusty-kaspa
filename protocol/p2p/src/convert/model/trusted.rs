//!
//! Model structures which are related to IBD pruning point syncing logic. These structures encode
//! a specific syncing protocol and thus do not belong within consensus core.
//!

use kaspa_consensus_core::{
    block::Block,
    blockhash::ORIGIN,
    trusted::{TrustedBlock, TrustedGhostdagData, TrustedHeader},
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
};

use crate::common::ProtocolError;

/// A package of *semi-trusted data* used by a syncing node in order to build
/// the sub-DAG in the anticone and in the recent past of the synced pruning point
pub struct TrustedDataPackage {
    pub daa_window: Vec<TrustedHeader>,
    pub ghostdag_window: Vec<TrustedGhostdagData>,
}

impl TrustedDataPackage {
    pub fn new(daa_window: Vec<TrustedHeader>, ghostdag_window: Vec<TrustedGhostdagData>) -> Self {
        Self { daa_window, ghostdag_window }
    }

    /// Returns the trusted set -- a sub-DAG in the anti-future of the pruning point which contains
    /// all the blocks and ghostdag data needed in order to validate the headers in the future of
    /// the pruning point
    pub fn build_trusted_subdag(self, entries: Vec<TrustedDataEntry>) -> Result<Vec<TrustedBlock>, ProtocolError> {
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
                    return Err(ProtocolError::Other("missing ghostdag data for some trusted entries"));
                }
            }
        }

        for th in self.daa_window.iter() {
            if set.insert(th.header.hash) {
                blocks.push(TrustedBlock::new(Block::from_header_arc(th.header.clone()), th.ghostdag.clone()));
            }
        }

        // Prune all missing ghostdag mergeset blocks. If due to this prune data becomes insufficient, future
        // IBD blocks will not validate correctly which will lead to a rule error and peer disconnection
        for tb in blocks.iter_mut() {
            tb.ghostdag.mergeset_blues.retain(|h| set.contains(h));
            tb.ghostdag.mergeset_reds.retain(|h| set.contains(h));
            tb.ghostdag.blues_anticone_sizes.retain(|k, _| set.contains(k));
            if !set.contains(&tb.ghostdag.selected_parent) {
                tb.ghostdag.selected_parent = ORIGIN;
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
    // is maintained now. Meaning we simply build this sub-DAG in a way that the usual traversal operations will
    // return the correct blocks/data without the need for explicitly provided indices.
    //
}

impl TrustedDataEntry {
    pub fn new(block: Block, daa_window_indices: Vec<u64>, ghostdag_window_indices: Vec<u64>) -> Self {
        Self { block, daa_window_indices, ghostdag_window_indices }
    }
}
