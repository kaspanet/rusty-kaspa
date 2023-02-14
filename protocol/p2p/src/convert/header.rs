use crate::pb as protowire;
use consensus_core::{header::Header, BlueWorkType};
use hashes::Hash;

use super::error::ConversionError;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Header> for protowire::BlockHeader {
    fn from(item: &Header) -> Self {
        Self {
            version: item.version.into(),
            parents: item.parents_by_level.iter().map(protowire::BlockLevelParents::from).collect(),
            hash_merkle_root: Some(item.hash_merkle_root.into()),
            accepted_id_merkle_root: Some(item.accepted_id_merkle_root.into()),
            utxo_commitment: Some(item.utxo_commitment.into()),
            timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            // We follow the golang specification of variable big-endian here
            blue_work: item.blue_work.to_be_bytes_var(),
            blue_score: item.blue_score,
            pruning_point: Some(item.pruning_point.into()),
        }
    }
}

impl From<&Vec<Hash>> for protowire::BlockLevelParents {
    fn from(item: &Vec<Hash>) -> Self {
        Self { parent_hashes: item.iter().map(|h| h.into()).collect() }
    }
}

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

impl TryFrom<&protowire::BlockHeader> for Header {
    type Error = ConversionError;
    fn try_from(item: &protowire::BlockHeader) -> Result<Self, Self::Error> {
        Ok(Self::new(
            item.version.try_into()?,
            item.parents.iter().map(Vec::<Hash>::try_from).collect::<Result<Vec<Vec<Hash>>, ConversionError>>()?,
            try_from_hash_op(&item.hash_merkle_root)?,
            try_from_hash_op(&item.accepted_id_merkle_root)?,
            try_from_hash_op(&item.utxo_commitment)?,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            item.daa_score,
            // We follow the golang specification of variable big-endian here
            BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            item.blue_score,
            try_from_hash_op(&item.pruning_point)?,
        ))
    }
}

impl TryFrom<&protowire::BlockLevelParents> for Vec<Hash> {
    type Error = ConversionError;
    fn try_from(item: &protowire::BlockLevelParents) -> Result<Self, Self::Error> {
        item.parent_hashes.iter().map(|x| x.try_into()).collect()
    }
}

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
