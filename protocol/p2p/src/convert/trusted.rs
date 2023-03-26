use kaspa_consensus_core::trusted::{TrustedGhostdagData, TrustedHeader};

use crate::pb as protowire;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&TrustedHeader> for protowire::DaaBlockV4 {
    fn from(item: &TrustedHeader) -> Self {
        Self { header: Some((&*item.header).into()), ghostdag_data: Some((&item.ghostdag).into()) }
    }
}

impl From<&TrustedGhostdagData> for protowire::BlockGhostdagDataHashPair {
    fn from(item: &TrustedGhostdagData) -> Self {
        Self { hash: Some(item.hash.into()), ghostdag_data: Some((&item.ghostdag).into()) }
    }
}
