use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{header::Header, BlueWorkType};
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

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
    pub hash: Hash,
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

impl RpcHeader {
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
            hash: header.hash,
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

impl From<&Header> for RpcHeader {
    fn from(header: &Header) -> Self {
        Self {
            hash: header.hash,
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

impl From<RpcHeader> for Header {
    fn from(header: RpcHeader) -> Self {
        Self {
            hash: header.hash,
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

impl From<&RpcHeader> for Header {
    fn from(header: &RpcHeader) -> Self {
        Self {
            hash: header.hash,
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

impl Serializer for RpcHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(Hash, &self.hash, writer)?;
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

impl Deserializer for RpcHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

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
