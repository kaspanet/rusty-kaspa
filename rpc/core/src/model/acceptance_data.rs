use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::RpcHash;

use super::{RpcOptionalHeader, RpcOptionalTransaction};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptanceData {
    /// struct properties are optionally returned depending on the verbosity level
    pub accepting_chain_header: Option<RpcOptionalHeader>,
    /// struct properties are optionally returned depending on the verbosity level
    pub mergeset_block_acceptance_data: Vec<RpcMergesetBlockAcceptanceData>,
}

impl RpcAcceptanceData {
    pub fn new(
        accepting_chain_header: Option<RpcOptionalHeader>,
        mergeset_block_acceptance_data: Vec<RpcMergesetBlockAcceptanceData>,
    ) -> Self {
        Self { accepting_chain_header, mergeset_block_acceptance_data }
    }
}

impl Serializer for RpcAcceptanceData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcOptionalHeader>, &self.accepting_chain_header, writer)?;
        serialize!(Vec<RpcMergesetBlockAcceptanceData>, &self.mergeset_block_acceptance_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcAcceptanceData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);
        let accepting_chain_header = load!(Option<RpcOptionalHeader>, reader)?;
        let mergeset_block_acceptance_data = deserialize!(Vec<RpcMergesetBlockAcceptanceData>, reader)?;

        Ok(Self { accepting_chain_header, mergeset_block_acceptance_data })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMergesetBlockAcceptanceData {
    pub merged_block_hash: RpcHash,
    pub accepted_transactions: Vec<RpcOptionalTransaction>,
}

impl RpcMergesetBlockAcceptanceData {
    #[inline(always)]
    pub fn new(merged_block_hash: RpcHash, accepted_transactions: Vec<RpcOptionalTransaction>) -> Self {
        Self { merged_block_hash, accepted_transactions }
    }
}

impl Serializer for RpcMergesetBlockAcceptanceData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;

        store!(Hash, &self.merged_block_hash, writer)?;
        serialize!(Vec<RpcOptionalTransaction>, &self.accepted_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcMergesetBlockAcceptanceData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);

        let merged_block_hash = load!(RpcHash, reader)?;
        let accepted_transactions = deserialize!(Vec<RpcOptionalTransaction>, reader)?;

        Ok(Self { merged_block_hash, accepted_transactions })
    }
}
