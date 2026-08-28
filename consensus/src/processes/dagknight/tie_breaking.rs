use kaspa_consensus_core::BlockHashSet;
use kaspa_hashes::Hash;

/// Chain blocks from a subgroup's conditioned k-colouring.
pub type SubgroupChainBlocks = Vec<Hash>;

/// Result of free-search k-colouring reference cluster computation.
pub struct ReferenceCluster {
    /// Set of blue block hashes in the resulting colouring.
    pub blues: BlockHashSet,
    /// Chain backbone from virtual towards conflict_genesis (inclusive).
    pub chain_blocks: Vec<Hash>,
}
