use crate::{block::Block, blockhash::BlockHashes, header::Header, BlockHashMap, BlueWorkType};
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
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these blocks are indirectly validated through the PoW mined over them
pub struct TrustedBlock {
    pub block: Block,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedBlock {
    pub fn new(block: Block, ghostdag: ExternalGhostdagData) -> Self {
        Self { block, ghostdag }
    }
}

/// Represent an externally provided header with associated Ghostdag data which
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these headers are indirectly validated through the PoW mined over them
pub struct TrustedHeader {
    pub header: Arc<Header>,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedHeader {
    pub fn new(header: Arc<Header>, ghostdag: ExternalGhostdagData) -> Self {
        Self { header, ghostdag }
    }
}

/// Represent externally provided Ghostdag data associated with a block Hash
pub struct TrustedHash {
    pub hash: Hash,
    pub ghostdag: ExternalGhostdagData,
}

impl TrustedHash {
    pub fn new(hash: Hash, ghostdag: ExternalGhostdagData) -> Self {
        Self { hash, ghostdag }
    }
}

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
}
