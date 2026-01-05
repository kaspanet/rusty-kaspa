use kaspa_consensus_core::trusted::{TrustedGhostdagData, TrustedHeader};

use crate::convert::header::HeaderFormat;
use crate::pb as protowire;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &TrustedHeader)> for protowire::DaaBlockV4 {
    fn from(value: (HeaderFormat, &TrustedHeader)) -> Self {
        let (header_format, item) = value;
        Self { header: Some((header_format, &*item.header).into()), ghostdag_data: Some((&item.ghostdag).into()) }
    }
}

impl From<&TrustedGhostdagData> for protowire::BlockGhostdagDataHashPair {
    fn from(item: &TrustedGhostdagData) -> Self {
        Self { hash: Some(item.hash.into()), ghostdag_data: Some((&item.ghostdag).into()) }
    }
}
