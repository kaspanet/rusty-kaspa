use crate::pb as protowire;
use kaspa_consensus_core::subnets::SubnetworkId;

use super::error::ConversionError;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<SubnetworkId> for protowire::SubnetworkId {
    fn from(item: SubnetworkId) -> Self {
        Self { bytes: <SubnetworkId as AsRef<[u8]>>::as_ref(&item).to_vec() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::SubnetworkId> for SubnetworkId {
    type Error = ConversionError;

    fn try_from(value: protowire::SubnetworkId) -> Result<Self, Self::Error> {
        Ok(value.bytes.as_slice().try_into()?)
    }
}
