use kaspa_consensus_core::trusted::{TrustedGhostdagData, TrustedHeader};

use crate::convert::header::HeaderFormat;
use crate::pb as protowire;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &TrustedHeader)> for protowire::DaaBlockV4 {
    fn from(value: (HeaderFormat, &TrustedHeader)) -> Self {
        let (header_format, item) = value;
        Self {
            header: Some((header_format, &*item.header).into()),
            coloring_ghostdag_data: Some((&item.coloring_ghostdag).into()),
            topology_ghostdag_data: Some((&item.topology_ghostdag).into()),
        }
    }
}

impl From<&TrustedGhostdagData> for protowire::BlockGhostdagDataHashPair {
    fn from(item: &TrustedGhostdagData) -> Self {
        Self {
            hash: Some(item.hash.into()),
            coloring_ghostdag_data: Some((&item.coloring_ghostdag).into()),
            topology_ghostdag_data: Some((&item.topology_ghostdag).into()),
        }
    }
}
