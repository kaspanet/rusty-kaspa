use super::error::ConversionError;
use crate::pb as protowire;
use hashes::Hash;

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

pub fn try_from_hash_op(hash: &Option<protowire::Hash>) -> Result<Hash, ConversionError> {
    if let Some(hash) = hash {
        Ok(hash.try_into()?)
    } else {
        Err(ConversionError::NoneHash)
    }
}

impl TryFrom<&protowire::Hash> for Hash {
    type Error = ConversionError;

    fn try_from(hash: &protowire::Hash) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(hash.bytes.as_slice().try_into()?))
    }
}
