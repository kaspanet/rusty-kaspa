use crate::{block::Block, blockhash::BlockHashes, BlockHashMap, BlueWorkType};
use hashes::Hash;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type KType = u8; // This type must be increased to u16 if we ever set GHOSTDAG K > 255
pub type HashKTypeMap = Arc<BlockHashMap<KType>>;

/// Represent externally provided Ghostdag data (by a network peer)
#[derive(Clone, Serialize, Deserialize)]
pub struct ExternalGhostdagData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: Hash,
    pub mergeset_blues: BlockHashes,
    pub mergeset_reds: BlockHashes,
    pub blues_anticone_sizes: HashKTypeMap,
}

/// Represent an externally provided block with associated Ghostdag data which
/// is only partially validated by the consensus layer. There is no actual trust
/// but rather these blocks are indirectly validated through the PoW mined over them
pub struct TrustedBlock {
    pub block: Block,
    pub ghostdag_data: ExternalGhostdagData,
}

impl TrustedBlock {
    pub fn new(block: Block, ghostdag_data: ExternalGhostdagData) -> Self {
        Self { block, ghostdag_data }
    }
}
