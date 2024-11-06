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
    relay_block_blue_work: BlueWorkType,
}

impl PruningProofMetadata {
    pub fn new(relay_block_blue_work: BlueWorkType) -> Self {
        Self { relay_block_blue_work }
    }

    /// The amount of blue work since the syncer's pruning point
    pub fn claimed_prover_relay_work(&self, pruning_point_work: BlueWorkType) -> BlueWorkType {
        if self.relay_block_blue_work <= pruning_point_work {
            return 0.into();
        }

        self.relay_block_blue_work - pruning_point_work
    }
}
