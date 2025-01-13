use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{header::Header, BlueWorkType};
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::{RpcError, RpcResult};

/// Raw Rpc header type - without a cached header hash.
/// Used for mining APIs (get_block_template & submit_block)
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRawHeader {
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub utxo_commitment: Hash,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeader {
    /// Cached hash
    pub hash: Option<Hash>,
    pub version: Option<u16>,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Option<Hash>,
    pub accepted_id_merkle_root: Option<Hash>,
    pub utxo_commitment: Option<Hash>,
    /// Timestamp is in milliseconds
    pub timestamp: Option<u64>,
    pub bits: Option<u32>,
    pub nonce: Option<u64>,
    pub daa_score: Option<u64>,
    pub blue_work: Option<BlueWorkType>,
    pub blue_score: Option<u64>,
    pub pruning_point: Option<Hash>,
}

impl RpcHeader {
    pub fn is_empty(&self) -> bool {
        self.hash.is_none()
            && self.version.is_none()
            && self.parents_by_level.is_empty()
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
    pub fn direct_parents(&self) -> &[Hash] {
        if self.parents_by_level.is_empty() {
            &[]
        } else {
            &self.parents_by_level[0]
        }
    }
}

impl AsRef<RpcHeader> for RpcHeader {
    fn as_ref(&self) -> &RpcHeader {
        self
    }
}

impl From<Header> for RpcHeader {
    fn from(header: Header) -> Self {
        Self {
            hash: Some(header.hash),
            version: Some(header.version),
            parents_by_level: header.parents_by_level,
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

impl From<&Header> for RpcHeader {
    fn from(header: &Header) -> Self {
        Self {
            hash: Some(header.hash),
            version: Some(header.version),
            parents_by_level: header.parents_by_level.clone(),
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

impl TryFrom<RpcHeader> for Header {
    type Error = RpcError;

    fn try_from(header: RpcHeader) -> RpcResult<Self> {
        Ok(Self {
            hash: header.hash.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash".to_owned()))?,
            version: header.version.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "version".to_owned()))?,
            parents_by_level: header.parents_by_level,
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

impl TryFrom<&RpcHeader> for Header {
    type Error = RpcError;

    fn try_from(header: &RpcHeader) -> RpcResult<Self> {
        Ok(Self {
            hash: header.hash.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "hash".to_owned()))?,
            version: header.version.ok_or(RpcError::MissingRpcFieldError("RpcHeader".to_owned(), "version".to_owned()))?,
            parents_by_level: header.parents_by_level.clone(),
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

impl Serializer for RpcHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;

        store!(Option<Hash>, &self.hash, writer)?;
        store!(Option<u16>, &self.version, writer)?;
        store!(Vec<Vec<Hash>>, &self.parents_by_level, writer)?;
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

impl Deserializer for RpcHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        match _version {
            1 => {
                let hash = load!(Hash, reader)?;
                let version = load!(u16, reader)?;
                let parents_by_level = load!(Vec<Vec<Hash>>, reader)?;
                let hash_merkle_root = load!(Hash, reader)?;
                let accepted_id_merkle_root = load!(Hash, reader)?;
                let utxo_commitment = load!(Hash, reader)?;
                let timestamp = load!(u64, reader)?;
                let bits = load!(u32, reader)?;
                let nonce = load!(u64, reader)?;
                let daa_score = load!(u64, reader)?;
                let blue_work = load!(BlueWorkType, reader)?;
                let blue_score = load!(u64, reader)?;
                let pruning_point = load!(Hash, reader)?;

                Ok(Self {
                    hash: Some(hash),
                    version: Some(version),
                    parents_by_level,
                    hash_merkle_root: Some(hash_merkle_root),
                    accepted_id_merkle_root: Some(accepted_id_merkle_root),
                    utxo_commitment: Some(utxo_commitment),
                    timestamp: Some(timestamp),
                    bits: Some(bits),
                    nonce: Some(nonce),
                    daa_score: Some(daa_score),
                    blue_work: Some(blue_work),
                    blue_score: Some(blue_score),
                    pruning_point: Some(pruning_point),
                })
            }
            2 => {
                let hash = load!(Option<Hash>, reader)?;
                let version = load!(Option<u16>, reader)?;
                let parents_by_level = load!(Vec<Vec<Hash>>, reader)?;
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
            _ => panic!("Unsupported version"),
        }
    }
}

impl From<RpcRawHeader> for Header {
    fn from(header: RpcRawHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level,
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&RpcRawHeader> for Header {
    fn from(header: &RpcRawHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level.clone(),
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&Header> for RpcRawHeader {
    fn from(header: &Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<Header> for RpcRawHeader {
    fn from(header: Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl Serializer for RpcRawHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(u16, &self.version, writer)?;
        store!(Vec<Vec<Hash>>, &self.parents_by_level, writer)?;
        store!(Hash, &self.hash_merkle_root, writer)?;
        store!(Hash, &self.accepted_id_merkle_root, writer)?;
        store!(Hash, &self.utxo_commitment, writer)?;
        store!(u64, &self.timestamp, writer)?;
        store!(u32, &self.bits, writer)?;
        store!(u64, &self.nonce, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(BlueWorkType, &self.blue_work, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(Hash, &self.pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcRawHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let version = load!(u16, reader)?;
        let parents_by_level = load!(Vec<Vec<Hash>>, reader)?;
        let hash_merkle_root = load!(Hash, reader)?;
        let accepted_id_merkle_root = load!(Hash, reader)?;
        let utxo_commitment = load!(Hash, reader)?;
        let timestamp = load!(u64, reader)?;
        let bits = load!(u32, reader)?;
        let nonce = load!(u64, reader)?;
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(Hash, reader)?;

        Ok(Self {
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

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeaderVerbosity {
    /// Cached hash
    pub include_hash: Option<bool>,
    pub include_version: Option<bool>,
    pub include_parents_by_level: Option<bool>,
    pub include_hash_merkle_root: Option<bool>,
    pub include_accepted_id_merkle_root: Option<bool>,
    pub include_utxo_commitment: Option<bool>,
    /// Timestamp is in milliseconds
    pub include_timestamp: Option<bool>,
    pub include_bits: Option<bool>,
    pub include_nonce: Option<bool>,
    pub include_daa_score: Option<bool>,
    pub include_blue_work: Option<bool>,
    pub include_blue_score: Option<bool>,
    pub include_pruning_point: Option<bool>,
}

impl Serializer for RpcHeaderVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(Option<bool>, &self.include_hash, writer)?;
        store!(Option<bool>, &self.include_version, writer)?;
        store!(Option<bool>, &self.include_parents_by_level, writer)?;
        store!(Option<bool>, &self.include_hash_merkle_root, writer)?;
        store!(Option<bool>, &self.include_accepted_id_merkle_root, writer)?;
        store!(Option<bool>, &self.include_utxo_commitment, writer)?;
        store!(Option<bool>, &self.include_timestamp, writer)?;
        store!(Option<bool>, &self.include_bits, writer)?;
        store!(Option<bool>, &self.include_nonce, writer)?;
        store!(Option<bool>, &self.include_daa_score, writer)?;
        store!(Option<bool>, &self.include_blue_work, writer)?;
        store!(Option<bool>, &self.include_blue_score, writer)?;
        store!(Option<bool>, &self.include_pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcHeaderVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let include_hash = load!(Option<bool>, reader)?;
        let include_version = load!(Option<bool>, reader)?;
        let include_parents_by_level = load!(Option<bool>, reader)?;
        let include_hash_merkle_root = load!(Option<bool>, reader)?;
        let include_accepted_id_merkle_root = load!(Option<bool>, reader)?;
        let include_utxo_commitment = load!(Option<bool>, reader)?;
        let include_timestamp = load!(Option<bool>, reader)?;
        let include_bits = load!(Option<bool>, reader)?;
        let include_nonce = load!(Option<bool>, reader)?;
        let include_daa_score = load!(Option<bool>, reader)?;
        let include_blue_work = load!(Option<bool>, reader)?;
        let include_blue_score = load!(Option<bool>, reader)?;
        let include_pruning_point = load!(Option<bool>, reader)?;

        Ok(Self {
            include_hash,
            include_version,
            include_parents_by_level,
            include_hash_merkle_root,
            include_accepted_id_merkle_root,
            include_utxo_commitment,
            include_timestamp,
            include_bits,
            include_nonce,
            include_daa_score,
            include_blue_work,
            include_blue_score,
            include_pruning_point,
        })
    }
}
