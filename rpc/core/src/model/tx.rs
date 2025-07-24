use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptVec, TransactionId, TransactionIndexType, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_utils::{hex::ToHex, serde_bytes_fixed_ref};
use serde::{Deserialize, Serialize};
use serde_nested_with::serde_nested;
use workflow_serializer::prelude::*;

use crate::{
    prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId},
    RpcError, RpcResult,
};

/// Represents the ID of a Kaspa transaction
pub type RpcTransactionId = TransactionId;

pub type RpcScriptVec = ScriptVec;
pub type RpcScriptPublicKey = ScriptPublicKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntry {
    pub amount: Option<u64>,
    pub script_public_key: Option<ScriptPublicKey>,
    pub block_daa_score: Option<u64>,
    pub is_coinbase: Option<bool>,
    pub verbose_data: Option<RpcUtxoEntryVerboseData>,
}

impl RpcUtxoEntry {
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
        verbose_data: Option<RpcUtxoEntryVerboseData>,
    ) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase, verbose_data }
    }
}

impl From<UtxoEntry> for RpcUtxoEntry {
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

impl TryFrom<RpcUtxoEntry> for UtxoEntry {
    type Error = RpcError;

    fn try_from(entry: RpcUtxoEntry) -> RpcResult<Self> {
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

impl Serializer for RpcUtxoEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(Option<u64>, &self.amount, writer)?;
        store!(Option<ScriptPublicKey>, &self.script_public_key, writer)?;
        store!(Option<u64>, &self.block_daa_score, writer)?;
        store!(Option<bool>, &self.is_coinbase, writer)?;
        serialize!(Option<RpcUtxoEntryVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        match _version {
            1 => {
                let amount = Some(load!(u64, reader)?);
                let script_public_key = Some(load!(ScriptPublicKey, reader)?);
                let block_daa_score = Some(load!(u64, reader)?);
                let is_coinbase = Some(load!(bool, reader)?);
                let verbose_data = None; // this field was not present in version 1

                Ok(Self { amount, script_public_key, block_daa_score, is_coinbase, verbose_data })
            }
            2 => {
                let amount = load!(Option<u64>, reader)?;
                let script_public_key = load!(Option<ScriptPublicKey>, reader)?;
                let block_daa_score = load!(Option<u64>, reader)?;
                let is_coinbase = load!(Option<bool>, reader)?;
                let verbose_data = deserialize!(Option<RpcUtxoEntryVerboseData>, reader)?;

                Ok(Self { amount, script_public_key, block_daa_score, is_coinbase, verbose_data })
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        }
    }
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntryVerboseData {
    pub script_public_key_type: Option<RpcScriptClass>,
    pub script_public_key_address: Option<Address>,
}

impl RpcUtxoEntryVerboseData {
    pub fn is_empty(&self) -> bool {
        self.script_public_key_type.is_none() && self.script_public_key_address.is_none()
    }

    pub fn new(script_public_key_type: Option<RpcScriptClass>, script_public_key_address: Option<Address>) -> Self {
        Self { script_public_key_type, script_public_key_address }
    }
}

impl Serializer for RpcUtxoEntryVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcScriptClass>, &self.script_public_key_type, writer)?;
        store!(Option<Address>, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntryVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let script_public_key_type = load!(Option<RpcScriptClass>, reader)?;
        let script_public_key_address = load!(Option<Address>, reader)?;

        Ok(Self { script_public_key_type, script_public_key_address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntryVerboseDataVerbosity {
    pub include_script_public_key_type: Option<bool>,
    pub include_script_public_key_address: Option<bool>,
}

impl RpcUtxoEntryVerboseDataVerbosity {
    pub fn new(include_script_public_key_type: Option<bool>, include_script_public_key_address: Option<bool>) -> Self {
        Self { include_script_public_key_type, include_script_public_key_address }
    }
}

impl Serializer for RpcUtxoEntryVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_script_public_key_type, writer)?;
        store!(Option<bool>, &self.include_script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntryVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let include_script_public_key_type = load!(Option<bool>, reader)?;
        let include_script_public_key_address = load!(Option<bool>, reader)?;

        Ok(Self { include_script_public_key_type, include_script_public_key_address })
    }
}

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutpoint {
    #[serde_nested(sub = "TransactionId", serde(with = "serde_bytes_fixed_ref"))]
    pub transaction_id: Option<TransactionId>,
    pub index: Option<TransactionIndexType>,
}

impl From<TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: TransactionOutpoint) -> Self {
        Self { transaction_id: Some(outpoint.transaction_id), index: Some(outpoint.index) }
    }
}

impl TryFrom<RpcTransactionOutpoint> for TransactionOutpoint {
    type Error = RpcError;

    fn try_from(outpoint: RpcTransactionOutpoint) -> RpcResult<Self> {
        Ok(Self {
            transaction_id: outpoint
                .transaction_id
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutpoint".to_string(), "transaction_id".to_string()))?,
            index: outpoint.index.ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutpoint".to_string(), "index".to_string()))?,
        })
    }
}

impl From<kaspa_consensus_client::TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: kaspa_consensus_client::TransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl TryFrom<RpcTransactionOutpoint> for kaspa_consensus_client::TransactionOutpoint {
    type Error = RpcError;

    fn try_from(outpoint: RpcTransactionOutpoint) -> RpcResult<Self> {
        Ok(TransactionOutpoint::try_from(outpoint)?.into())
    }
}

impl Serializer for RpcTransactionOutpoint {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(Option<TransactionId>, &self.transaction_id, writer)?;
        store!(Option<TransactionIndexType>, &self.index, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutpoint {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        match _version {
            1 => {
                let transaction_id = Some(load!(TransactionId, reader)?);
                let index = Some(load!(TransactionIndexType, reader)?);
                Ok(Self { transaction_id, index })
            }
            2 => {
                let transaction_id = load!(Option<TransactionId>, reader)?;
                let index = load!(Option<TransactionIndexType>, reader)?;
                Ok(Self { transaction_id, index })
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        }
    }
}

/// Represents a Kaspa transaction input
#[derive(Clone, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: Option<RpcTransactionOutpoint>,
    #[serde_nested(sub = "Vec<u8>", serde(with = "hex::serde"))]
    pub signature_script: Option<Vec<u8>>,
    pub sequence: Option<u64>,
    pub sig_op_count: Option<u8>,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

impl std::fmt::Debug for RpcTransactionInput {
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

impl From<TransactionInput> for RpcTransactionInput {
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

impl RpcTransactionInput {
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

impl Serializer for RpcTransactionInput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.previous_outpoint, writer)?;
        store!(Option<Vec<u8>>, &self.signature_script, writer)?;
        store!(Option<u64>, &self.sequence, writer)?;
        store!(Option<u8>, &self.sig_op_count, writer)?;
        serialize!(Option<RpcTransactionInputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        Ok(match _version {
            1 => {
                let previous_outpoint = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
                let signature_script = Some(load!(Vec<u8>, reader)?);
                let sequence = Some(load!(u64, reader)?);
                let sig_op_count = Some(load!(u8, reader)?);
                let verbose_data = deserialize!(Option<RpcTransactionInputVerboseData>, reader)?;

                Self { previous_outpoint, signature_script, sequence, sig_op_count, verbose_data }
            }
            2 => {
                let previous_outpoint = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
                let signature_script = load!(Option<Vec<u8>>, reader)?;
                let sequence = load!(Option<u64>, reader)?;
                let sig_op_count = load!(Option<u8>, reader)?;
                let verbose_data = deserialize!(Option<RpcTransactionInputVerboseData>, reader)?;

                Self { previous_outpoint, signature_script, sequence, sig_op_count, verbose_data }
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        })
    }
}

/// Represent Kaspa transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {
    pub utxo_entry: Option<RpcUtxoEntry>,
}

impl RpcTransactionInputVerboseData {
    pub fn is_empty(&self) -> bool {
        self.utxo_entry.is_none() || self.utxo_entry.as_ref().is_some_and(|x| x.is_empty())
    }
}

impl Serializer for RpcTransactionInputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcUtxoEntry>, &self.utxo_entry, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let utxo_entry = deserialize!(Option<RpcUtxoEntry>, reader)?;
        Ok(Self { utxo_entry })
    }
}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub value: Option<u64>,
    pub script_public_key: Option<RpcScriptPublicKey>,
    pub verbose_data: Option<RpcTransactionOutputVerboseData>,
}

impl RpcTransactionOutput {
    pub fn is_empty(&self) -> bool {
        self.value.is_none()
            && self.script_public_key.is_none()
            && (self.verbose_data.is_none() || self.verbose_data.as_ref().is_some_and(|x| x.is_empty()))
    }

    pub fn from_transaction_outputs(other: Vec<TransactionOutput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }
}

impl From<TransactionOutput> for RpcTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self { value: Some(output.value), script_public_key: Some(output.script_public_key), verbose_data: None }
    }
}

impl Serializer for RpcTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(Option<u64>, &self.value, writer)?;
        store!(Option<RpcScriptPublicKey>, &self.script_public_key, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(match _version {
            1 => {
                let value = Some(load!(u64, reader)?);
                let script_public_key = Some(load!(RpcScriptPublicKey, reader)?);
                let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;

                Self { value, script_public_key, verbose_data }
            }
            2 => {
                let value = load!(Option<u64>, reader)?;
                let script_public_key = load!(Option<RpcScriptPublicKey>, reader)?;
                let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;

                Self { value, script_public_key, verbose_data }
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        })
    }
}

/// Represent Kaspa transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: Option<RpcScriptClass>,
    pub script_public_key_address: Option<Address>,
}

impl RpcTransactionOutputVerboseData {
    pub fn is_empty(&self) -> bool {
        self.script_public_key_type.is_none() && self.script_public_key_address.is_none()
    }
}

impl Serializer for RpcTransactionOutputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(Option<RpcScriptClass>, &self.script_public_key_type, writer)?;
        store!(Option<Address>, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(match _version {
            1 => {
                let script_public_key_type = Some(load!(RpcScriptClass, reader)?);
                let script_public_key_address = Some(load!(Address, reader)?);
                Self { script_public_key_type, script_public_key_address }
            }
            2 => {
                let script_public_key_type = load!(Option<RpcScriptClass>, reader)?;
                let script_public_key_address = load!(Option<Address>, reader)?;
                Self { script_public_key_type, script_public_key_address }
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        })
    }
}

/// Represents a Kaspa transaction
#[derive(Clone, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: Option<u16>,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    pub lock_time: Option<u64>,
    pub subnetwork_id: Option<RpcSubnetworkId>,
    pub gas: Option<u64>,
    #[serde_nested(sub = "Vec<u8>", serde(with = "hex::serde"))]
    pub payload: Option<Vec<u8>>,
    pub mass: Option<u64>,
    pub verbose_data: Option<RpcTransactionVerboseData>,
}

impl RpcTransaction {
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

impl std::fmt::Debug for RpcTransaction {
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

impl Serializer for RpcTransaction {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(Option<u16>, &self.version, writer)?;
        serialize!(Vec<RpcTransactionInput>, &self.inputs, writer)?;
        serialize!(Vec<RpcTransactionOutput>, &self.outputs, writer)?;
        store!(Option<u64>, &self.lock_time, writer)?;
        store!(Option<RpcSubnetworkId>, &self.subnetwork_id, writer)?;
        store!(Option<u64>, &self.gas, writer)?;
        store!(Option<Vec<u8>>, &self.payload, writer)?;
        store!(Option<u64>, &self.mass, writer)?;
        serialize!(Option<RpcTransactionVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransaction {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _struct_version = load!(u16, reader)?;
        Ok(match _struct_version {
            1 => {
                let version = Some(load!(u16, reader)?);
                let inputs = deserialize!(Vec<RpcTransactionInput>, reader)?;
                let outputs = deserialize!(Vec<RpcTransactionOutput>, reader)?;
                let lock_time = Some(load!(u64, reader)?);
                let subnetwork_id = Some(load!(RpcSubnetworkId, reader)?);
                let gas = Some(load!(u64, reader)?);
                let payload = Some(load!(Vec<u8>, reader)?);
                let mass = Some(load!(u64, reader)?);
                let verbose_data = deserialize!(Option<RpcTransactionVerboseData>, reader)?;

                Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, verbose_data }
            }
            2 => {
                let version = load!(Option<u16>, reader)?;
                let inputs = deserialize!(Vec<RpcTransactionInput>, reader)?;
                let outputs = deserialize!(Vec<RpcTransactionOutput>, reader)?;
                let lock_time = load!(Option<u64>, reader)?;
                let subnetwork_id = load!(Option<RpcSubnetworkId>, reader)?;
                let gas = load!(Option<u64>, reader)?;
                let payload = load!(Option<Vec<u8>>, reader)?;
                let mass = load!(Option<u64>, reader)?;
                let verbose_data = deserialize!(Option<RpcTransactionVerboseData>, reader)?;

                Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, verbose_data }
            }
            _ => {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _struct_version)))
            }
        })
    }
}

/// Represent Kaspa transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde_nested]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    #[serde_nested(sub = "RpcTransactionId", serde(with = "serde_bytes_fixed_ref"))]
    pub transaction_id: Option<RpcTransactionId>,
    #[serde_nested(sub = "RpcHash", serde(with = "serde_bytes_fixed_ref"))]
    pub hash: Option<RpcHash>,
    pub compute_mass: Option<u64>,
    #[serde_nested(sub = "RpcHash", serde(with = "serde_bytes_fixed_ref"))]
    pub block_hash: Option<RpcHash>,
    pub block_time: Option<u64>,
}

impl RpcTransactionVerboseData {
    pub fn is_empty(&self) -> bool {
        self.transaction_id.is_none()
            && self.hash.is_none()
            && self.compute_mass.is_none()
            && self.block_hash.is_none()
            && self.block_time.is_none()
    }
}

impl Serializer for RpcTransactionVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(Option<RpcTransactionId>, &self.transaction_id, writer)?;
        store!(Option<RpcHash>, &self.hash, writer)?;
        store!(Option<u64>, &self.compute_mass, writer)?;
        store!(Option<RpcHash>, &self.block_hash, writer)?;
        store!(Option<u64>, &self.block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(match _version {
            1 => {
                let transaction_id = Some(load!(RpcTransactionId, reader)?);
                let hash = Some(load!(RpcHash, reader)?);
                let compute_mass = Some(load!(u64, reader)?);
                let block_hash = Some(load!(RpcHash, reader)?);
                let block_time = Some(load!(u64, reader)?);

                Self { transaction_id, hash, compute_mass, block_hash, block_time }
            }
            2 => {
                let transaction_id = load!(Option<RpcTransactionId>, reader)?;
                let hash = load!(Option<RpcHash>, reader)?;
                let compute_mass = load!(Option<u64>, reader)?;
                let block_hash = load!(Option<RpcHash>, reader)?;
                let block_time = load!(Option<u64>, reader)?;

                Self { transaction_id, hash, compute_mass, block_hash, block_time }
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported version: {}", _version))),
        })
    }
}

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptedTransactionIds {
    #[serde(with = "serde_bytes_fixed_ref")]
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}

// RpcUtxoEntryVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntryVerbosity {
    pub include_amount: Option<bool>,
    pub include_script_public_key: Option<bool>,
    pub include_block_daa_score: Option<bool>,
    pub include_is_coinbase: Option<bool>,
    pub verbose_data_verbosity: Option<RpcUtxoEntryVerboseDataVerbosity>,
}

impl RpcUtxoEntryVerbosity {
    pub fn new(
        include_amount: Option<bool>,
        include_script_public_key: Option<bool>,
        include_block_daa_score: Option<bool>,
        include_is_coinbase: Option<bool>,
        verbose_data_verbosity: Option<RpcUtxoEntryVerboseDataVerbosity>,
    ) -> Self {
        Self { include_amount, include_script_public_key, include_block_daa_score, include_is_coinbase, verbose_data_verbosity }
    }
}

impl Serializer for RpcUtxoEntryVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_amount, writer)?;
        store!(Option<bool>, &self.include_script_public_key, writer)?;
        store!(Option<bool>, &self.include_block_daa_score, writer)?;
        store!(Option<bool>, &self.include_is_coinbase, writer)?;
        serialize!(Option<RpcUtxoEntryVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntryVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_amount = load!(Option<bool>, reader)?;
        let include_script_public_key = load!(Option<bool>, reader)?;
        let include_block_daa_score = load!(Option<bool>, reader)?;
        let include_is_coinbase = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcUtxoEntryVerboseDataVerbosity>, reader)?;

        Ok(Self { include_amount, include_script_public_key, include_block_daa_score, include_is_coinbase, verbose_data_verbosity })
    }
}

// RpcTransactionInputVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerbosity {
    pub include_previous_outpoint: Option<bool>,
    pub include_signature_script: Option<bool>,
    pub include_sequence: Option<bool>,
    pub include_sig_op_count: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionInputVerboseDataVerbosity>,
}

impl RpcTransactionInputVerbosity {
    pub fn new(
        include_previous_outpoint: Option<bool>,
        include_signature_script: Option<bool>,
        include_sequence: Option<bool>,
        include_sig_op_count: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionInputVerboseDataVerbosity>,
    ) -> Self {
        Self { include_previous_outpoint, include_signature_script, include_sequence, include_sig_op_count, verbose_data_verbosity }
    }
}

impl Serializer for RpcTransactionInputVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_previous_outpoint, writer)?;
        store!(Option<bool>, &self.include_signature_script, writer)?;
        store!(Option<bool>, &self.include_sequence, writer)?;
        store!(Option<bool>, &self.include_sig_op_count, writer)?;
        serialize!(Option<RpcTransactionInputVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_previous_outpoint = load!(Option<bool>, reader)?;
        let include_signature_script = load!(Option<bool>, reader)?;
        let include_sequence = load!(Option<bool>, reader)?;
        let include_sig_op_count = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionInputVerboseDataVerbosity>, reader)?;

        Ok(Self {
            include_previous_outpoint,
            include_signature_script,
            include_sequence,
            include_sig_op_count,
            verbose_data_verbosity,
        })
    }
}

// RpcTransactionInputVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseDataVerbosity {
    pub utxo_entry_verbosity: Option<RpcUtxoEntryVerbosity>,
}

impl RpcTransactionInputVerboseDataVerbosity {
    pub fn new(utxo_entry_verbosity: Option<RpcUtxoEntryVerbosity>) -> Self {
        Self { utxo_entry_verbosity }
    }
}

impl Serializer for RpcTransactionInputVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcUtxoEntryVerbosity>, &self.utxo_entry_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let utxo_entry_verbosity = deserialize!(Option<RpcUtxoEntryVerbosity>, reader)?;

        Ok(Self { utxo_entry_verbosity })
    }
}

// RpcTransactionOutputVerbosity

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerbosity {
    pub include_amount: Option<bool>,
    pub include_script_public_key: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionOutputVerboseDataVerbosity>,
}

impl RpcTransactionOutputVerbosity {
    pub fn new(
        include_amount: Option<bool>,
        include_script_public_key: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionOutputVerboseDataVerbosity>,
    ) -> Self {
        Self { include_amount, include_script_public_key, verbose_data_verbosity }
    }
}

impl Serializer for RpcTransactionOutputVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_amount, writer)?;
        store!(Option<bool>, &self.include_script_public_key, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_amount = load!(Option<bool>, reader)?;
        let include_script_public_key = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionOutputVerboseDataVerbosity>, reader)?;

        Ok(Self { include_amount, include_script_public_key, verbose_data_verbosity })
    }
}

// RpcTransactionOutputVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseDataVerbosity {
    pub include_script_public_key_type: Option<bool>,
    pub include_script_public_key_address: Option<bool>,
}

impl RpcTransactionOutputVerboseDataVerbosity {
    pub fn new(include_script_public_key_type: Option<bool>, include_script_public_key_address: Option<bool>) -> Self {
        Self { include_script_public_key_type, include_script_public_key_address }
    }
}

impl Serializer for RpcTransactionOutputVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_script_public_key_type, writer)?;
        store!(Option<bool>, &self.include_script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_script_public_key_type = load!(Option<bool>, reader)?;
        let include_script_public_key_address = load!(Option<bool>, reader)?;

        Ok(Self { include_script_public_key_type, include_script_public_key_address })
    }
}

// RpcTransactionVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerbosity {
    pub include_version: Option<bool>,
    pub input_verbosity: Option<RpcTransactionInputVerbosity>,
    pub output_verbosity: Option<RpcTransactionOutputVerbosity>,
    pub include_lock_time: Option<bool>,
    pub include_subnetwork_id: Option<bool>,
    pub include_gas: Option<bool>,
    pub include_payload: Option<bool>,
    pub include_mass: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionVerboseDataVerbosity>,
}

impl RpcTransactionVerbosity {
    pub fn new(
        include_version: Option<bool>,
        input_verbosity: Option<RpcTransactionInputVerbosity>,
        output_verbosity: Option<RpcTransactionOutputVerbosity>,
        include_lock_time: Option<bool>,
        include_subnetwork_id: Option<bool>,
        include_gas: Option<bool>,
        include_payload: Option<bool>,
        include_mass: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionVerboseDataVerbosity>,
    ) -> Self {
        Self {
            include_version,
            input_verbosity,
            output_verbosity,
            include_lock_time,
            include_subnetwork_id,
            include_gas,
            include_payload,
            include_mass,
            verbose_data_verbosity,
        }
    }

    pub fn requires_populated_transaction(&self) -> bool {
        self.input_verbosity
            .as_ref()
            .is_some_and(|active| active.verbose_data_verbosity.as_ref().is_some_and(|active| active.utxo_entry_verbosity.is_some()))
    }

    pub fn requires_block_hash(&self) -> bool {
        self.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.is_some())
    }

    pub fn requires_block_time(&self) -> bool {
        self.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_time.is_some())
    }
}

impl Serializer for RpcTransactionVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_version, writer)?;
        serialize!(Option<RpcTransactionInputVerbosity>, &self.input_verbosity, writer)?;
        serialize!(Option<RpcTransactionOutputVerbosity>, &self.output_verbosity, writer)?;
        store!(Option<bool>, &self.include_lock_time, writer)?;
        store!(Option<bool>, &self.include_subnetwork_id, writer)?;
        store!(Option<bool>, &self.include_gas, writer)?;
        store!(Option<bool>, &self.include_payload, writer)?;
        store!(Option<bool>, &self.include_mass, writer)?;
        serialize!(Option<RpcTransactionVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_version = load!(Option<bool>, reader)?;
        let input_verbosity = deserialize!(Option<RpcTransactionInputVerbosity>, reader)?;
        let output_verbosity = deserialize!(Option<RpcTransactionOutputVerbosity>, reader)?;
        let include_lock_time = load!(Option<bool>, reader)?;
        let include_subnetwork_id = load!(Option<bool>, reader)?;
        let include_gas = load!(Option<bool>, reader)?;
        let include_payload = load!(Option<bool>, reader)?;
        let include_mass = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionVerboseDataVerbosity>, reader)?;

        Ok(Self {
            include_version,
            input_verbosity,
            output_verbosity,
            include_lock_time,
            include_subnetwork_id,
            include_gas,
            include_payload,
            include_mass,
            verbose_data_verbosity,
        })
    }
}

// RpcTransactionVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseDataVerbosity {
    pub include_transaction_id: Option<bool>,
    pub include_hash: Option<bool>,
    pub include_compute_mass: Option<bool>,
    pub include_block_hash: Option<bool>,
    pub include_block_time: Option<bool>,
}

impl RpcTransactionVerboseDataVerbosity {
    pub fn new(
        include_transaction_id: Option<bool>,
        include_hash: Option<bool>,
        include_compute_mass: Option<bool>,
        include_block_hash: Option<bool>,
        include_block_time: Option<bool>,
    ) -> Self {
        Self { include_transaction_id, include_hash, include_compute_mass, include_block_hash, include_block_time }
    }
}

impl Serializer for RpcTransactionVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_transaction_id, writer)?;
        store!(Option<bool>, &self.include_hash, writer)?;
        store!(Option<bool>, &self.include_compute_mass, writer)?;
        store!(Option<bool>, &self.include_block_hash, writer)?;
        store!(Option<bool>, &self.include_block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_transaction_id = load!(Option<bool>, reader)?;
        let include_hash = load!(Option<bool>, reader)?;
        let include_compute_mass = load!(Option<bool>, reader)?;
        let include_block_hash = load!(Option<bool>, reader)?;
        let include_block_time = load!(Option<bool>, reader)?;

        Ok(Self { include_transaction_id, include_hash, include_compute_mass, include_block_hash, include_block_time })
    }
}
