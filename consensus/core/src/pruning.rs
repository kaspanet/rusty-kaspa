use crate::{
    header::Header,
    trusted::{TrustedGhostdagData, TrustedHeader},
    BlueWorkType,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

pub type PruningPointProof = Vec<Vec<Arc<Header>>>;

pub type PruningPointsList = Vec<Arc<Header>>;

pub struct PruningPointTrustedData {
    /// The pruning point anticone from virtual PoV
    pub anticone: Vec<Hash>,

    /// Union of DAA window data required to verify blocks in the future of the pruning point
    pub daa_window_blocks: Vec<TrustedHeader>,

    /// Union of GHOSTDAG data required to verify blocks in the future of the pruning point
    pub ghostdag_blocks: Vec<TrustedGhostdagData>,
}

#[derive(Clone, Copy)]
pub struct PruningProofMetadata {
    /// The claimed work of the initial relay block (from the prover)
    pub relay_block_blue_work: BlueWorkType,
}

impl PruningProofMetadata {
    pub fn new(relay_block_blue_work: BlueWorkType) -> Self {
        Self { relay_block_blue_work }
    }
}
