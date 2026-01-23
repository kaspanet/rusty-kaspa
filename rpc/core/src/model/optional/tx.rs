use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, TransactionId, TransactionIndexType, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_utils::{hex::ToHex, serde_bytes_fixed_ref};
use serde::{Deserialize, Serialize};
use serde_nested_with::serde_nested;
use workflow_serializer::prelude::*;

use crate::{
    RpcError, RpcResult, RpcScriptPublicKey, RpcTransactionId,
    prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalUtxoEntry {
    /// Level: High
    pub amount: Option<u64>,
    /// Level: High
    pub script_public_key: Option<ScriptPublicKey>,
    /// Level: Full
    pub block_daa_score: Option<u64>,
    /// Level: High
    pub is_coinbase: Option<bool>,
    pub verbose_data: Option<RpcOptionalUtxoEntryVerboseData>,
}

impl RpcOptionalUtxoEntry {
    pub fn is_empty(&self) -> bool {
        self.amount.is_none()
            && self.script_public_key.is_none()
            && self.block_daa_score.is_none()
            && self.is_coinbase.is_none()
            && (self.verbose_data.is_none() || self.verbose_data.as_ref().is_some_and(|x| x.is_empty()))
    }

    pub fn new(
        amount: Option<u64>,
        script_public_key: Option<ScriptPublicKey>,
        block_daa_score: Option<u64>,
        is_coinbase: Option<bool>,
        verbose_data: Option<RpcOptionalUtxoEntryVerboseData>,
    ) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase, verbose_data }
    }
}

impl From<UtxoEntry> for RpcOptionalUtxoEntry {
    fn from(entry: UtxoEntry) -> Self {
        Self {
            amount: Some(entry.amount),
            script_public_key: Some(entry.script_public_key),
            block_daa_score: Some(entry.block_daa_score),
            is_coinbase: Some(entry.is_coinbase),
            verbose_data: None,
        }
    }
}

impl TryFrom<RpcOptionalUtxoEntry> for UtxoEntry {
    type Error = RpcError;

    fn try_from(entry: RpcOptionalUtxoEntry) -> RpcResult<Self> {
        Ok(Self {
            amount: entry.amount.ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "amount".to_string()))?,
            script_public_key: entry
                .script_public_key
                .ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "script_public_key".to_string()))?,
            block_daa_score: entry
                .block_daa_score
                .ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "block_daa_score".to_string()))?,
            is_coinbase: entry
                .is_coinbase
                .ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "is_coinbase".to_string()))?,
        })
    }
}

impl Serializer for RpcOptionalUtxoEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<u64>, &self.amount, writer)?;
        store!(Option<ScriptPublicKey>, &self.script_public_key, writer)?;
        store!(Option<u64>, &self.block_daa_score, writer)?;
        store!(Option<bool>, &self.is_coinbase, writer)?;
        serialize!(Option<RpcOptionalUtxoEntryVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalUtxoEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let amount = load!(Option<u64>, reader)?;
        let script_public_key = load!(Option<ScriptPublicKey>, reader)?;
        let block_daa_score = load!(Option<u64>, reader)?;
        let is_coinbase = load!(Option<bool>, reader)?;
        let verbose_data = deserialize!(Option<RpcOptionalUtxoEntryVerboseData>, reader)?;

        Ok(Self { amount, script_public_key, block_daa_score, is_coinbase, verbose_data })
    }
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalUtxoEntryVerboseData {
    /// Level: Low
    pub script_public_key_type: Option<RpcScriptClass>,
    /// Level: Low
    pub script_public_key_address: Option<Address>,
}

impl RpcOptionalUtxoEntryVerboseData {
    pub fn is_empty(&self) -> bool {
        self.script_public_key_type.is_none() && self.script_public_key_address.is_none()
    }

    pub fn new(script_public_key_type: Option<RpcScriptClass>, script_public_key_address: Option<Address>) -> Self {
        Self { script_public_key_type, script_public_key_address }
    }
}

impl Serializer for RpcOptionalUtxoEntryVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcScriptClass>, &self.script_public_key_type, writer)?;
        store!(Option<Address>, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalUtxoEntryVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let script_public_key_type = load!(Option<RpcScriptClass>, reader)?;
        let script_public_key_address = load!(Option<Address>, reader)?;

        Ok(Self { script_public_key_type, script_public_key_address })
    }
}

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionOutpoint {
    #[serde_nested(sub = "TransactionId", serde(with = "serde_bytes_fixed_ref"))]
    pub transaction_id: Option<TransactionId>,
    pub index: Option<TransactionIndexType>,
}

impl From<TransactionOutpoint> for RpcOptionalTransactionOutpoint {
    fn from(outpoint: TransactionOutpoint) -> Self {
        Self { transaction_id: Some(outpoint.transaction_id), index: Some(outpoint.index) }
    }
}

impl TryFrom<RpcOptionalTransactionOutpoint> for TransactionOutpoint {
    type Error = RpcError;

    fn try_from(outpoint: RpcOptionalTransactionOutpoint) -> RpcResult<Self> {
        Ok(Self {
            transaction_id: outpoint
                .transaction_id
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutpoint".to_string(), "transaction_id".to_string()))?,
            index: outpoint.index.ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutpoint".to_string(), "index".to_string()))?,
        })
    }
}

impl From<kaspa_consensus_client::TransactionOutpoint> for RpcOptionalTransactionOutpoint {
    fn from(outpoint: kaspa_consensus_client::TransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl TryFrom<RpcOptionalTransactionOutpoint> for kaspa_consensus_client::TransactionOutpoint {
    type Error = RpcError;

    fn try_from(outpoint: RpcOptionalTransactionOutpoint) -> RpcResult<Self> {
        Ok(TransactionOutpoint::try_from(outpoint)?.into())
    }
}

impl Serializer for RpcOptionalTransactionOutpoint {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<TransactionId>, &self.transaction_id, writer)?;
        store!(Option<TransactionIndexType>, &self.index, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionOutpoint {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(Option<TransactionId>, reader)?;
        let index = load!(Option<TransactionIndexType>, reader)?;

        Ok(Self { transaction_id, index })
    }
}

/// Represents a Kaspa transaction input
#[derive(Clone, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionInput {
    /// Level: High
    pub previous_outpoint: Option<RpcOptionalTransactionOutpoint>,
    #[serde_nested(sub = "Vec<u8>", serde(with = "hex::serde"))]
    /// Level: Low
    pub signature_script: Option<Vec<u8>>,
    /// Level: High
    pub sequence: Option<u64>,
    /// Level: High
    pub sig_op_count: Option<u8>,
    pub verbose_data: Option<RpcOptionalTransactionInputVerboseData>,
}

impl std::fmt::Debug for RpcOptionalTransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &self.signature_script.as_ref().map(|v| v.to_hex()))
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
}

impl From<TransactionInput> for RpcOptionalTransactionInput {
    fn from(input: TransactionInput) -> Self {
        Self {
            previous_outpoint: Some(input.previous_outpoint.into()),
            signature_script: Some(input.signature_script),
            sequence: Some(input.sequence),
            sig_op_count: Some(input.sig_op_count),
            verbose_data: None,
        }
    }
}

impl RpcOptionalTransactionInput {
    /// Note: verbose data will not be automatically populated when converting from TransactionInput to RpcTransactionInput
    pub fn from_transaction_inputs(other: Vec<TransactionInput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.previous_outpoint.is_none()
            && self.signature_script.is_none()
            && self.sequence.is_none()
            && self.sig_op_count.is_none()
            && (self.verbose_data.is_none() || self.verbose_data.as_ref().is_some_and(|x| x.is_empty()))
    }
}

impl Serializer for RpcOptionalTransactionInput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcOptionalTransactionOutpoint>, &self.previous_outpoint, writer)?;
        store!(Option<Vec<u8>>, &self.signature_script, writer)?;
        store!(Option<u64>, &self.sequence, writer)?;
        store!(Option<u8>, &self.sig_op_count, writer)?;
        serialize!(Option<RpcOptionalTransactionInputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionInput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let previous_outpoint = deserialize!(Option<RpcOptionalTransactionOutpoint>, reader)?;
        let signature_script = load!(Option<Vec<u8>>, reader)?;
        let sequence = load!(Option<u64>, reader)?;
        let sig_op_count = load!(Option<u8>, reader)?;
        let verbose_data = deserialize!(Option<RpcOptionalTransactionInputVerboseData>, reader)?;

        Ok(Self { previous_outpoint, signature_script, sequence, sig_op_count, verbose_data })
    }
}

/// Represent Kaspa transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionInputVerboseData {
    pub utxo_entry: Option<RpcOptionalUtxoEntry>,
}

impl RpcOptionalTransactionInputVerboseData {
    pub fn is_empty(&self) -> bool {
        self.utxo_entry.is_none() || self.utxo_entry.as_ref().is_some_and(|x| x.is_empty())
    }
}

impl Serializer for RpcOptionalTransactionInputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcOptionalUtxoEntry>, &self.utxo_entry, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionInputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let utxo_entry = deserialize!(Option<RpcOptionalUtxoEntry>, reader)?;
        Ok(Self { utxo_entry })
    }
}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionOutput {
    /// Level - Low
    pub value: Option<u64>,
    /// Level - Low
    pub script_public_key: Option<RpcScriptPublicKey>,
    pub verbose_data: Option<RpcOptionalTransactionOutputVerboseData>,
}

impl RpcOptionalTransactionOutput {
    pub fn is_empty(&self) -> bool {
        self.value.is_none()
            && self.script_public_key.is_none()
            && (self.verbose_data.is_none() || self.verbose_data.as_ref().is_some_and(|x| x.is_empty()))
    }

    pub fn from_transaction_outputs(other: Vec<TransactionOutput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }
}

impl From<TransactionOutput> for RpcOptionalTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self { value: Some(output.value), script_public_key: Some(output.script_public_key), verbose_data: None }
    }
}

impl Serializer for RpcOptionalTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<u64>, &self.value, writer)?;
        store!(Option<RpcScriptPublicKey>, &self.script_public_key, writer)?;
        serialize!(Option<RpcOptionalTransactionOutputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionOutput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let value = load!(Option<u64>, reader)?;
        let script_public_key = load!(Option<RpcScriptPublicKey>, reader)?;
        let verbose_data = deserialize!(Option<RpcOptionalTransactionOutputVerboseData>, reader)?;

        Ok(Self { value, script_public_key, verbose_data })
    }
}

/// Represent Kaspa transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionOutputVerboseData {
    /// Level: Low
    pub script_public_key_type: Option<RpcScriptClass>,
    /// Level: Low
    pub script_public_key_address: Option<Address>,
}

impl RpcOptionalTransactionOutputVerboseData {
    pub fn is_empty(&self) -> bool {
        self.script_public_key_type.is_none() && self.script_public_key_address.is_none()
    }
}

impl Serializer for RpcOptionalTransactionOutputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcScriptClass>, &self.script_public_key_type, writer)?;
        store!(Option<Address>, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionOutputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let script_public_key_type = load!(Option<RpcScriptClass>, reader)?;
        let script_public_key_address = load!(Option<Address>, reader)?;
        Ok(Self { script_public_key_type, script_public_key_address })
    }
}

/// Represents a Kaspa transaction
#[derive(Clone, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransaction {
    /// Level: Full
    pub version: Option<u16>,
    pub inputs: Vec<RpcOptionalTransactionInput>,
    pub outputs: Vec<RpcOptionalTransactionOutput>,
    /// Level: Full
    pub lock_time: Option<u64>,
    /// Level: Full
    pub subnetwork_id: Option<RpcSubnetworkId>,
    /// Level: Full
    pub gas: Option<u64>,
    #[serde_nested(sub = "Vec<u8>", serde(with = "hex::serde"))]
    /// Level: High
    pub payload: Option<Vec<u8>>,
    /// Level: High
    pub mass: Option<u64>,
    pub verbose_data: Option<RpcOptionalTransactionVerboseData>,
}

impl RpcOptionalTransaction {
    pub fn is_empty(&self) -> bool {
        self.version.is_none()
            && (self.inputs.is_empty() || self.inputs.iter().all(|input| input.is_empty()))
            && (self.outputs.is_empty() || self.outputs.iter().all(|output| output.is_empty()))
            && self.lock_time.is_none()
            && self.subnetwork_id.is_none()
            && self.gas.is_none()
            && self.payload.is_none()
            && self.mass.is_none()
            && (self.verbose_data.is_none() || self.verbose_data.as_ref().is_some_and(|x| x.is_empty()))
    }
}

impl std::fmt::Debug for RpcOptionalTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransaction")
            .field("version", &self.version)
            .field("lock_time", &self.lock_time)
            .field("subnetwork_id", &self.subnetwork_id)
            .field("gas", &self.gas)
            .field("payload", &self.payload.as_ref().map(|v|v.to_hex()))
            .field("mass", &self.mass)
            .field("inputs", &self.inputs) // Inputs and outputs are placed purposely at the end for better debug visibility
            .field("outputs", &self.outputs)
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
}

impl Serializer for RpcOptionalTransaction {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<u16>, &self.version, writer)?;
        serialize!(Vec<RpcOptionalTransactionInput>, &self.inputs, writer)?;
        serialize!(Vec<RpcOptionalTransactionOutput>, &self.outputs, writer)?;
        store!(Option<u64>, &self.lock_time, writer)?;
        store!(Option<RpcSubnetworkId>, &self.subnetwork_id, writer)?;
        store!(Option<u64>, &self.gas, writer)?;
        store!(Option<Vec<u8>>, &self.payload, writer)?;
        store!(Option<u64>, &self.mass, writer)?;
        serialize!(Option<RpcOptionalTransactionVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransaction {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _struct_version = load!(u16, reader)?;

        let version = load!(Option<u16>, reader)?;
        let inputs = deserialize!(Vec<RpcOptionalTransactionInput>, reader)?;
        let outputs = deserialize!(Vec<RpcOptionalTransactionOutput>, reader)?;
        let lock_time = load!(Option<u64>, reader)?;
        let subnetwork_id = load!(Option<RpcSubnetworkId>, reader)?;
        let gas = load!(Option<u64>, reader)?;
        let payload = load!(Option<Vec<u8>>, reader)?;
        let mass = load!(Option<u64>, reader)?;
        let verbose_data = deserialize!(Option<RpcOptionalTransactionVerboseData>, reader)?;

        Ok(Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, verbose_data })
    }
}

/// Represent Kaspa transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalTransactionVerboseData {
    #[serde_nested(sub = "RpcTransactionId", serde(with = "serde_bytes_fixed_ref"))]
    /// Level: Low
    pub transaction_id: Option<RpcTransactionId>,
    #[serde_nested(sub = "RpcHash", serde(with = "serde_bytes_fixed_ref"))]
    /// Level: Low
    pub hash: Option<RpcHash>,
    /// Level: High
    pub compute_mass: Option<u64>,
    #[serde_nested(sub = "RpcHash", serde(with = "serde_bytes_fixed_ref"))]
    /// Level: Low
    pub block_hash: Option<RpcHash>,
    /// Level: Low
    pub block_time: Option<u64>,
}

impl RpcOptionalTransactionVerboseData {
    pub fn is_empty(&self) -> bool {
        self.transaction_id.is_none()
            && self.hash.is_none()
            && self.compute_mass.is_none()
            && self.block_hash.is_none()
            && self.block_time.is_none()
    }
}

impl Serializer for RpcOptionalTransactionVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcTransactionId>, &self.transaction_id, writer)?;
        store!(Option<RpcHash>, &self.hash, writer)?;
        store!(Option<u64>, &self.compute_mass, writer)?;
        store!(Option<RpcHash>, &self.block_hash, writer)?;
        store!(Option<u64>, &self.block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalTransactionVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(Option<RpcTransactionId>, reader)?;
        let hash = load!(Option<RpcHash>, reader)?;
        let compute_mass = load!(Option<u64>, reader)?;
        let block_hash = load!(Option<RpcHash>, reader)?;
        let block_time = load!(Option<u64>, reader)?;

        Ok(Self { transaction_id, hash, compute_mass, block_hash, block_time })
    }
}

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalAcceptedTransactionIds {
    #[serde(with = "serde_bytes_fixed_ref")]
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}
