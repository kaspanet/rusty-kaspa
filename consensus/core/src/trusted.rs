use crate::{block::Block, blockhash::BlockHashes, header::Header, BlueWorkType, HashKTypeMap};
use hashes::Hash;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represent semi-trusted externally provided Ghostdag data (by a network peer)
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
