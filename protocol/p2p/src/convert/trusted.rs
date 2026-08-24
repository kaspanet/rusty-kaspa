use kaspa_consensus_core::{
    header::Header,
    trusted::{TrustedGhostdagData, TrustedHeader},
};
use std::sync::Arc;

use crate::pb as protowire;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&TrustedHeader> for protowire::DaaBlockV4 {
    fn from(item: &TrustedHeader) -> Self {
        Self {
            header: Some((&*item.header).into()),
            coloring_ghostdag_data: Some((&item.coloring_ghostdag).into()),
            topology_ghostdag_data: Some((&item.topology_ghostdag).into()),
        }
    }
}

impl From<&Arc<Header>> for protowire::DaaBlockV4 {
    fn from(header: &Arc<Header>) -> Self {
        Self { header: Some((&**header).into()), coloring_ghostdag_data: None, topology_ghostdag_data: None }
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
