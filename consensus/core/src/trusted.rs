use crate::{BlockHashMap, BlueWorkType, KType, block::Block, header::Header};
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents semi-trusted externally provided Ghostdag data (by a network peer)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalGhostdagData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: Hash,
    pub mergeset_blues: Vec<Hash>,
    pub mergeset_reds: Vec<Hash>,
    pub blues_anticone_sizes: BlockHashMap<KType>,
}

/// Represents an externally provided block with associated Ghostdag data which
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these blocks are indirectly validated through the PoW mined over them
#[derive(Clone)]
pub struct TrustedBlock {
    pub block: Block,
    pub coloring_ghostdag: ExternalGhostdagData,
    pub topology_ghostdag: ExternalGhostdagData,
}

impl TrustedBlock {
    pub fn new(block: Block, coloring_ghostdag: ExternalGhostdagData, topology_ghostdag: ExternalGhostdagData) -> Self {
        Self { block, coloring_ghostdag, topology_ghostdag }
    }
}

/// Represents an externally provided header with associated Ghostdag data which
/// is only partially validated by the consensus layer. Note there is no actual trust
/// but rather these headers are indirectly validated through the PoW mined over them
pub struct TrustedHeader {
    pub header: Arc<Header>,
    pub coloring_ghostdag: ExternalGhostdagData,
    pub topology_ghostdag: ExternalGhostdagData,
}

impl TrustedHeader {
    pub fn new(header: Arc<Header>, coloring_ghostdag: ExternalGhostdagData, topology_ghostdag: ExternalGhostdagData) -> Self {
        Self { header, coloring_ghostdag, topology_ghostdag }
    }
}

/// Represents externally provided Ghostdag data associated with a block Hash
pub struct TrustedGhostdagData {
    pub hash: Hash,
    pub coloring_ghostdag: ExternalGhostdagData,
    pub topology_ghostdag: ExternalGhostdagData,
}

impl TrustedGhostdagData {
    pub fn new(hash: Hash, coloring_ghostdag: ExternalGhostdagData, topology_ghostdag: ExternalGhostdagData) -> Self {
        Self { hash, coloring_ghostdag, topology_ghostdag }
    }
}
