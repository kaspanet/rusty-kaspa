use super::error::ConversionError;
use crate::pb as protowire;
use kaspa_consensus_core::header::Header;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Vec<Arc<Header>>> for protowire::PruningPointProofHeaderArray {
    fn from(v: &Vec<Arc<Header>>) -> Self {
        Self { headers: v.iter().map(|header| header.as_ref().into()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::PruningPointProofHeaderArray> for Vec<Arc<Header>> {
    type Error = ConversionError;

    fn try_from(v: protowire::PruningPointProofHeaderArray) -> Result<Self, Self::Error> {
        v.headers.into_iter().map(|x| x.try_into().map(Arc::new)).collect()
    }
}
