use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use super::{RpcHeader, RpcHeaderVerbosity, RpcTransaction, RpcTransactionVerbosity};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptanceData {
    pub accepting_chain_header: Option<RpcHeader>,
    pub mergeset_block_acceptance_data: Vec<RpcMergesetBlockAcceptanceData>,
}

impl RpcAcceptanceData {
    pub fn new(
        accepting_chain_header: Option<RpcHeader>,
        mergeset_block_acceptance_data: Vec<RpcMergesetBlockAcceptanceData>,
    ) -> Self {
        Self { accepting_chain_header, mergeset_block_acceptance_data }
    }
}

impl Serializer for RpcAcceptanceData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcHeader>, &self.accepting_chain_header, writer)?;
        serialize!(Vec<RpcMergesetBlockAcceptanceData>, &self.mergeset_block_acceptance_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcAcceptanceData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);
        let accepting_chain_header = load!(Option<RpcHeader>, reader)?;
        let mergeset_block_acceptance_data = deserialize!(Vec<RpcMergesetBlockAcceptanceData>, reader)?;

        Ok(Self { accepting_chain_header, mergeset_block_acceptance_data })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMergesetBlockAcceptanceData {
    pub merged_header: Option<RpcHeader>,
    pub accepted_transactions: Vec<RpcTransaction>,
}

impl RpcMergesetBlockAcceptanceData {
    #[inline(always)]
    pub fn new(merged_header: Option<RpcHeader>, accepted_transactions: Vec<RpcTransaction>) -> Self {
        Self { merged_header, accepted_transactions }
    }
}

impl Serializer for RpcMergesetBlockAcceptanceData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;

        store!(Option<RpcHeader>, &self.merged_header, writer)?;
        serialize!(Vec<RpcTransaction>, &self.accepted_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcMergesetBlockAcceptanceData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);

        let merged_header = load!(Option<RpcHeader>, reader)?;
        let accepted_transactions = deserialize!(Vec<RpcTransaction>, reader)?;

        Ok(Self { merged_header, accepted_transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptanceDataVerbosity {
    pub accepting_chain_header_verbosity: Option<RpcHeaderVerbosity>,
    pub mergeset_block_acceptance_data_verbosity: Option<RpcMergesetBlockAcceptanceDataVerbosity>,
}

impl RpcAcceptanceDataVerbosity {
    pub fn new(
        accepting_chain_header_verbosity: Option<RpcHeaderVerbosity>,
        mergeset_block_acceptance_data_verbosity: Option<RpcMergesetBlockAcceptanceDataVerbosity>,
    ) -> Self {
        Self { accepting_chain_header_verbosity, mergeset_block_acceptance_data_verbosity }
    }

    pub fn requires_merged_header(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity.as_ref().map_or(default, |active| active.requires_merged_header())
    }

    pub fn requeires_accepted_header(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity.as_ref().map_or(default, |active| active.requires_merged_block_hash())
    }

    pub fn requires_accepted_transactions(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity
            .as_ref()
            .map_or(default, |active| active.accepted_transactions_verbosity.is_some())
    }
}

impl Serializer for RpcAcceptanceDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcHeaderVerbosity>, &self.accepting_chain_header_verbosity, writer)?;
        serialize!(Option<RpcMergesetBlockAcceptanceDataVerbosity>, &self.mergeset_block_acceptance_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcAcceptanceDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);
        let accepting_chain_header_verbosity = load!(Option<RpcHeaderVerbosity>, reader)?;
        let mergeset_block_acceptance_data_verbosity = deserialize!(Option<RpcMergesetBlockAcceptanceDataVerbosity>, reader)?;

        Ok(Self { accepting_chain_header_verbosity, mergeset_block_acceptance_data_verbosity })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMergesetBlockAcceptanceDataVerbosity {
    pub merged_header_verbosity: Option<RpcHeaderVerbosity>,
    pub accepted_transactions_verbosity: Option<RpcTransactionVerbosity>,
}

impl RpcMergesetBlockAcceptanceDataVerbosity {
    pub fn requires_merged_header(&self) -> bool {
        self.merged_header_verbosity.is_some()
            || self.accepted_transactions_verbosity.as_ref().is_some_and(|active| {
                active.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.unwrap_or(false))
            })
    }

    pub fn requires_merged_block_hash(&self) -> bool {
        self.merged_header_verbosity.as_ref().is_some_and(|active| active.include_hash.unwrap_or(false))
            || self.accepted_transactions_verbosity.as_ref().is_some_and(|active| {
                active.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.unwrap_or(false))
            })
    }
}

impl RpcMergesetBlockAcceptanceDataVerbosity {
    pub fn new(
        merged_header_verbosity: Option<RpcHeaderVerbosity>,
        accepted_transactions_verbosity: Option<RpcTransactionVerbosity>,
    ) -> Self {
        Self { merged_header_verbosity, accepted_transactions_verbosity }
    }
}

impl Serializer for RpcMergesetBlockAcceptanceDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcHeaderVerbosity>, &self.merged_header_verbosity, writer)?;
        serialize!(Option<RpcTransactionVerbosity>, &self.accepted_transactions_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcMergesetBlockAcceptanceDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let merged_header_verbosity = deserialize!(Option<RpcHeaderVerbosity>, reader)?;
        let accepted_transactions_verbosity = deserialize!(Option<RpcTransactionVerbosity>, reader)?;

        Ok(Self { merged_header_verbosity, accepted_transactions_verbosity })
    }
}
