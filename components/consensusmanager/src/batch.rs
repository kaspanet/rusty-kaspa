use kaspa_consensus_core::{api::BlockValidationFuture, block::Block};
use std::fmt::Debug;

pub struct BlockProcessingBatch {
    pub blocks: Vec<Block>,
    pub block_tasks: Option<Vec<BlockValidationFuture>>,
    pub virtual_state_tasks: Option<Vec<BlockValidationFuture>>,
}

impl BlockProcessingBatch {
    pub fn new(blocks: Vec<Block>, block_tasks: Vec<BlockValidationFuture>, virtual_state_tasks: Vec<BlockValidationFuture>) -> Self {
        Self { blocks, block_tasks: Some(block_tasks), virtual_state_tasks: Some(virtual_state_tasks) }
    }

    pub fn zip(self) -> impl Iterator<Item = (Block, BlockValidationFuture)> {
        self.blocks.into_iter().zip(self.virtual_state_tasks.unwrap())
    }
}

impl Default for BlockProcessingBatch {
    fn default() -> Self {
        Self { blocks: Default::default(), block_tasks: Some(Default::default()), virtual_state_tasks: Some(Default::default()) }
    }
}

impl Debug for BlockProcessingBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockProcessingBatch").field("blocks", &self.blocks).finish()
    }
}
