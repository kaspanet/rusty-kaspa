use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{BlueWorkType, header::Header};
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::{RpcCompressedParents, RpcError, RpcResult};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalHeader {
    /// Level: None - Cached hash
    pub hash: Option<Hash>,
    /// Level: Low
    pub version: Option<u16>,
    /// Level: High
    pub parents_by_level: Option<RpcCompressedParents>,
    /// Level: High
    pub hash_merkle_root: Option<Hash>,
    /// Level: High
    pub accepted_id_merkle_root: Option<Hash>,
    /// Level: Full
    pub utxo_commitment: Option<Hash>,
    /// Level: Low - Timestamp is in milliseconds
    pub timestamp: Option<u64>,
    /// Level: Low
    pub bits: Option<u32>,
    /// Level: Low
    pub nonce: Option<u64>,
    /// Level: Low
    pub daa_score: Option<u64>,
    /// Level: Low
    pub blue_work: Option<BlueWorkType>,
    /// Level: Low
    pub blue_score: Option<u64>,
    /// Level: Full
    pub pruning_point: Option<Hash>,
}

impl RpcOptionalHeader {
    pub fn is_empty(&self) -> bool {
        self.hash.is_none()
            && self.version.is_none()
            && self.parents_by_level.is_none()
            && self.hash_merkle_root.is_none()
            && self.accepted_id_merkle_root.is_none()
            && self.utxo_commitment.is_none()
            && self.timestamp.is_none()
            && self.bits.is_none()
            && self.nonce.is_none()
            && self.daa_score.is_none()
            && self.blue_work.is_none()
            && self.blue_score.is_none()
            && self.pruning_point.is_none()
    }
}

impl AsRef<RpcOptionalHeader> for RpcOptionalHeader {
    fn as_ref(&self) -> &RpcOptionalHeader {
        self
    }
}

impl From<Header> for RpcOptionalHeader {
    fn from(header: Header) -> Self {
        Self {
            hash: Some(header.hash),
            version: Some(header.version),
            parents_by_level: Some(header.parents_by_level),
            hash_merkle_root: Some(header.hash_merkle_root),
            accepted_id_merkle_root: Some(header.accepted_id_merkle_root),
            utxo_commitment: Some(header.utxo_commitment),
            timestamp: Some(header.timestamp),
            bits: Some(header.bits),
            nonce: Some(header.nonce),
            daa_score: Some(header.daa_score),
            blue_work: Some(header.blue_work),
            blue_score: Some(header.blue_score),
            pruning_point: Some(header.pruning_point),
        }
    }
}

impl From<&Header> for RpcOptionalHeader {
    fn from(header: &Header) -> Self {
        Self {
            hash: Some(header.hash),
            version: Some(header.version),
            parents_by_level: Some(header.parents_by_level.clone()),
            hash_merkle_root: Some(header.hash_merkle_root),
            accepted_id_merkle_root: Some(header.accepted_id_merkle_root),
            utxo_commitment: Some(header.utxo_commitment),
            timestamp: Some(header.timestamp),
            bits: Some(header.bits),
            nonce: Some(header.nonce),
            daa_score: Some(header.daa_score),
            blue_work: Some(header.blue_work),
            blue_score: Some(header.blue_score),
            pruning_point: Some(header.pruning_point),
        }
    }
}

impl TryFrom<RpcOptionalHeader> for Header {
    type Error = RpcError;

    fn try_from(header: RpcOptionalHeader) -> RpcResult<Self> {
        Ok(Self {
            hash: header.hash.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash".to_owned()))?,
            version: header.version.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "version".to_owned()))?,
            parents_by_level: header
                .parents_by_level
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "parents_by_level".to_owned()))?,
            hash_merkle_root: header
                .hash_merkle_root
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash_merkle_root".to_owned()))?,
            accepted_id_merkle_root: header
                .accepted_id_merkle_root
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "accepted_id_merkle_root".to_owned()))?,
            utxo_commitment: header
                .utxo_commitment
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "utxo_commitment".to_owned()))?,
            timestamp: header.timestamp.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "timestamp".to_owned()))?,
            bits: header.bits.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "bits".to_owned()))?,
            nonce: header.nonce.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "nonce".to_owned()))?,
            daa_score: header.daa_score.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "daa_score".to_owned()))?,
            blue_work: header.blue_work.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "blue_work".to_owned()))?,
            blue_score: header.blue_score.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "blue_score".to_owned()))?,
            pruning_point: header
                .pruning_point
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "pruning_point".to_owned()))?,
        })
    }
}

impl TryFrom<&RpcOptionalHeader> for Header {
    type Error = RpcError;

    fn try_from(header: &RpcOptionalHeader) -> RpcResult<Self> {
        Ok(Self {
            hash: header.hash.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash".to_owned()))?,
            version: header.version.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "version".to_owned()))?,
            parents_by_level: header
                .parents_by_level
                .clone()
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "parents_by_level".to_owned()))?,
            hash_merkle_root: header
                .hash_merkle_root
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash_merkle_root".to_owned()))?,
            accepted_id_merkle_root: header
                .accepted_id_merkle_root
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "accepted_id_merkle_root".to_owned()))?,
            utxo_commitment: header
                .utxo_commitment
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "utxo_commitment".to_owned()))?,
            timestamp: header.timestamp.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "timestamp".to_owned()))?,
            bits: header.bits.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "bits".to_owned()))?,
            nonce: header.nonce.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "nonce".to_owned()))?,
            daa_score: header.daa_score.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "daa_score".to_owned()))?,
            blue_work: header.blue_work.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "blue_work".to_owned()))?,
            blue_score: header.blue_score.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "blue_score".to_owned()))?,
            pruning_point: header
                .pruning_point
                .ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "pruning_point".to_owned()))?,
        })
    }
}

impl Serializer for RpcOptionalHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(Option<Hash>, &self.hash, writer)?;
        store!(Option<u16>, &self.version, writer)?;
        store!(Option<RpcCompressedParents>, &self.parents_by_level, writer)?;
        store!(Option<Hash>, &self.hash_merkle_root, writer)?;
        store!(Option<Hash>, &self.accepted_id_merkle_root, writer)?;
        store!(Option<Hash>, &self.utxo_commitment, writer)?;
        store!(Option<u64>, &self.timestamp, writer)?;
        store!(Option<u32>, &self.bits, writer)?;
        store!(Option<u64>, &self.nonce, writer)?;
        store!(Option<u64>, &self.daa_score, writer)?;
        store!(Option<BlueWorkType>, &self.blue_work, writer)?;
        store!(Option<u64>, &self.blue_score, writer)?;
        store!(Option<Hash>, &self.pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let hash = load!(Option<Hash>, reader)?;
        let version = load!(Option<u16>, reader)?;
        let parents_by_level = load!(Option<RpcCompressedParents>, reader)?;
        let hash_merkle_root = load!(Option<Hash>, reader)?;
        let accepted_id_merkle_root = load!(Option<Hash>, reader)?;
        let utxo_commitment = load!(Option<Hash>, reader)?;
        let timestamp = load!(Option<u64>, reader)?;
        let bits = load!(Option<u32>, reader)?;
        let nonce = load!(Option<u64>, reader)?;
        let daa_score = load!(Option<u64>, reader)?;
        let blue_work = load!(Option<BlueWorkType>, reader)?;
        let blue_score = load!(Option<u64>, reader)?;
        let pruning_point = load!(Option<Hash>, reader)?;

        Ok(Self {
            hash,
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            timestamp,
            bits,
            nonce,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
        })
    }
}
