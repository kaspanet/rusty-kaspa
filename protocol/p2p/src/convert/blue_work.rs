use super::error::ConversionError;
use crate::pb as protowire;
use kaspa_consensus_core::BlueWorkType;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<BlueWorkType> for protowire::BlueWork {
    fn from(blue_work: BlueWorkType) -> Self {
        Self { bytes: blue_work.to_be_bytes_var() }
    }
}

impl From<&BlueWorkType> for protowire::BlueWork {
    fn from(blue_work: &BlueWorkType) -> Self {
        Self { bytes: blue_work.to_be_bytes_var() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BlueWork> for BlueWorkType {
    type Error = ConversionError;
    fn try_from(blue_work: protowire::BlueWork) -> Result<Self, Self::Error> {
        Ok(BlueWorkType::from_be_bytes_var(&blue_work.bytes)?)
    }
}
