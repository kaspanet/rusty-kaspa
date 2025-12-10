use super::error::ConversionError;
use crate::convert::header::{HeaderFormat, Versioned};
use crate::pb as protowire;
use kaspa_consensus_core::header::Header;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &Vec<Arc<Header>>)> for protowire::PruningPointProofHeaderArray {
    fn from(value: (HeaderFormat, &Vec<Arc<Header>>)) -> Self {
        let (header_format, v) = value;
        Self { headers: v.iter().map(|header| (header_format, header.as_ref()).into()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<Versioned<protowire::PruningPointProofHeaderArray>> for Vec<Arc<Header>> {
    type Error = ConversionError;

    fn try_from(value: Versioned<protowire::PruningPointProofHeaderArray>) -> Result<Self, Self::Error> {
        let Versioned(header_format, v) = value;
        v.headers.into_iter().map(|x| Versioned(header_format, x).try_into().map(Arc::new)).collect()
    }
}
