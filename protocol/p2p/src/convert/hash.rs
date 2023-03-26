use super::error::ConversionError;
use crate::pb as protowire;
use kaspa_hashes::Hash;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<Hash> for protowire::Hash {
    fn from(hash: Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&Hash> for protowire::Hash {
    fn from(hash: &Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::Hash> for Hash {
    type Error = ConversionError;

    fn try_from(hash: protowire::Hash) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(hash.bytes.as_slice().try_into()?))
    }
}
