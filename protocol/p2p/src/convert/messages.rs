use super::error::ConversionError;
use crate::pb::{PruningPointProofMessage, PruningPointsMessage};
use consensus_core::{header::Header, pruning::PruningPointProof};
use std::sync::Arc;

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<PruningPointProofMessage> for PruningPointProof {
    type Error = ConversionError;
    fn try_from(msg: PruningPointProofMessage) -> Result<Self, Self::Error> {
        msg.headers.iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<PruningPointsMessage> for Vec<Arc<Header>> {
    type Error = ConversionError;
    fn try_from(msg: PruningPointsMessage) -> Result<Self, Self::Error> {
        msg.headers.iter().map(|x| x.try_into().map(Arc::new)).collect()
    }
}
