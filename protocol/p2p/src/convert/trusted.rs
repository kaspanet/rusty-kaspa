use kaspa_consensus_core::{
    header::Header,
    trusted::{TrustedGhostdagData, TrustedHeader},
};
use std::sync::Arc;

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

impl From<(HeaderFormat, &Arc<Header>)> for protowire::DaaBlockV4 {
    fn from(value: (HeaderFormat, &Arc<Header>)) -> Self {
        let (header_format, header) = value;
        Self { header: Some((header_format, &**header).into()), ghostdag_data: None }
    }
}

impl From<&TrustedGhostdagData> for protowire::BlockGhostdagDataHashPair {
    fn from(item: &TrustedGhostdagData) -> Self {
        Self { hash: Some(item.hash.into()), ghostdag_data: Some((&item.ghostdag).into()) }
    }
}
