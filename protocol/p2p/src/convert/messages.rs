use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use consensus_core::{
    ghostdag::{TrustedDataEntry, TrustedDataPackage},
    pruning::{PruningPointProof, PruningPointsList},
};
use std::sync::Arc;

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::PruningPointProofMessage> for PruningPointProof {
    type Error = ConversionError;
    fn try_from(msg: protowire::PruningPointProofMessage) -> Result<Self, Self::Error> {
        msg.headers.iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::PruningPointsMessage> for PruningPointsList {
    type Error = ConversionError;
    fn try_from(msg: protowire::PruningPointsMessage) -> Result<Self, Self::Error> {
        msg.headers.iter().map(|x| x.try_into().map(Arc::new)).collect()
    }
}

impl TryFrom<protowire::TrustedDataMessage> for TrustedDataPackage {
    type Error = ConversionError;
    fn try_from(msg: protowire::TrustedDataMessage) -> Result<Self, Self::Error> {
        Ok(Self::new(
            msg.daa_window.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, Self::Error>>()?,
            msg.ghostdag_data.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, Self::Error>>()?,
        ))
    }
}

impl TryFrom<protowire::BlockWithTrustedDataV4Message> for TrustedDataEntry {
    type Error = ConversionError;

    fn try_from(msg: protowire::BlockWithTrustedDataV4Message) -> Result<Self, Self::Error> {
        Ok(Self::new((&msg.block).try_into_ex()?, msg.daa_window_indices, msg.ghostdag_data_indices))
    }
}
