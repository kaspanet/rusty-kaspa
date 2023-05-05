use crate::{
    header::Header,
    trusted::{TrustedGhostdagData, TrustedHeader},
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
