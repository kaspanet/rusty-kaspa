use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptVec, TransactionId, TransactionIndexType, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_utils::serde_bytes_fixed_ref;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId};

/// Represents the ID of a Kaspa transaction
pub type RpcTransactionId = TransactionId;

pub type RpcScriptVec = ScriptVec;
pub type RpcScriptPublicKey = ScriptPublicKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntry {
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl RpcUtxoEntry {
    pub fn new(amount: u64, script_public_key: ScriptPublicKey, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase }
    }
}

impl From<UtxoEntry> for RpcUtxoEntry {
    fn from(entry: UtxoEntry) -> Self {
        Self {
            amount: entry.amount,
            script_public_key: entry.script_public_key,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}

impl From<RpcUtxoEntry> for UtxoEntry {
    fn from(entry: RpcUtxoEntry) -> Self {
        Self {
            amount: entry.amount,
            script_public_key: entry.script_public_key,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}

impl Serializer for RpcUtxoEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(ScriptPublicKey, &self.script_public_key, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(bool, &self.is_coinbase, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let amount = load!(u64, reader)?;
        let script_public_key = load!(ScriptPublicKey, reader)?;
        let block_daa_score = load!(u64, reader)?;
        let is_coinbase = load!(bool, reader)?;

        Ok(Self { amount, script_public_key, block_daa_score, is_coinbase })
    }
}

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutpoint {
    #[serde(with = "serde_bytes_fixed_ref")]
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl From<TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: TransactionOutpoint) -> Self {
        Self { transaction_id: outpoint.transaction_id, index: outpoint.index }
    }
}

impl From<RpcTransactionOutpoint> for TransactionOutpoint {
    fn from(outpoint: RpcTransactionOutpoint) -> Self {
        Self { transaction_id: outpoint.transaction_id, index: outpoint.index }
    }
}

impl From<kaspa_consensus_client::TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: kaspa_consensus_client::TransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl From<RpcTransactionOutpoint> for kaspa_consensus_client::TransactionOutpoint {
    fn from(outpoint: RpcTransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl Serializer for RpcTransactionOutpoint {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(TransactionId, &self.transaction_id, writer)?;
        store!(TransactionIndexType, &self.index, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutpoint {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(TransactionId, reader)?;
        let index = load!(TransactionIndexType, reader)?;

        Ok(Self { transaction_id, index })
    }
}

/// Represents a Kaspa transaction input
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: RpcTransactionOutpoint,
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

impl From<TransactionInput> for RpcTransactionInput {
    fn from(input: TransactionInput) -> Self {
        Self {
            previous_outpoint: input.previous_outpoint.into(),
            signature_script: input.signature_script,
            sequence: input.sequence,
            sig_op_count: input.sig_op_count,
            verbose_data: None,
        }
    }
}

impl RpcTransactionInput {
    pub fn from_transaction_inputs(other: Vec<TransactionInput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }
}

impl Serializer for RpcTransactionInput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(RpcTransactionOutpoint, &self.previous_outpoint, writer)?;
        store!(Vec<u8>, &self.signature_script, writer)?;
        store!(u64, &self.sequence, writer)?;
        store!(u8, &self.sig_op_count, writer)?;
        serialize!(Option<RpcTransactionInputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let previous_outpoint = deserialize!(RpcTransactionOutpoint, reader)?;
        let signature_script = load!(Vec<u8>, reader)?;
        let sequence = load!(u64, reader)?;
        let sig_op_count = load!(u8, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionInputVerboseData>, reader)?;

        Ok(Self { previous_outpoint, signature_script, sequence, sig_op_count, verbose_data })
    }
}

/// Represent Kaspa transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {}

impl Serializer for RpcTransactionInputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(Self {})
    }
}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub value: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub verbose_data: Option<RpcTransactionOutputVerboseData>,
}

impl RpcTransactionOutput {
    pub fn from_transaction_outputs(other: Vec<TransactionOutput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }
}

impl From<TransactionOutput> for RpcTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key, verbose_data: None }
    }
}

impl Serializer for RpcTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(u64, &self.value, writer)?;
        store!(RpcScriptPublicKey, &self.script_public_key, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let value = load!(u64, reader)?;
        let script_public_key = load!(RpcScriptPublicKey, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;

        Ok(Self { value, script_public_key, verbose_data })
    }
}

/// Represent Kaspa transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: RpcScriptClass,
    pub script_public_key_address: Address,
}

impl Serializer for RpcTransactionOutputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(RpcScriptClass, &self.script_public_key_type, writer)?;
        store!(Address, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let script_public_key_type = load!(RpcScriptClass, reader)?;
        let script_public_key_address = load!(Address, reader)?;

        Ok(Self { script_public_key_type, script_public_key_address })
    }
}

/// Represents a Kaspa transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: u16,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: RpcSubnetworkId,
    pub gas: u64,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
    pub mass: u64,
    pub verbose_data: Option<RpcTransactionVerboseData>,
}

impl Serializer for RpcTransaction {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u16, &self.version, writer)?;
        serialize!(Vec<RpcTransactionInput>, &self.inputs, writer)?;
        serialize!(Vec<RpcTransactionOutput>, &self.outputs, writer)?;
        store!(u64, &self.lock_time, writer)?;
        store!(RpcSubnetworkId, &self.subnetwork_id, writer)?;
        store!(u64, &self.gas, writer)?;
        store!(Vec<u8>, &self.payload, writer)?;
        store!(u64, &self.mass, writer)?;
        serialize!(Option<RpcTransactionVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransaction {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _struct_version = load!(u16, reader)?;
        let version = load!(u16, reader)?;
        let inputs = deserialize!(Vec<RpcTransactionInput>, reader)?;
        let outputs = deserialize!(Vec<RpcTransactionOutput>, reader)?;
        let lock_time = load!(u64, reader)?;
        let subnetwork_id = load!(RpcSubnetworkId, reader)?;
        let gas = load!(u64, reader)?;
        let payload = load!(Vec<u8>, reader)?;
        let mass = load!(u64, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionVerboseData>, reader)?;

        Ok(Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, verbose_data })
    }
}

/// Represent Kaspa transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    pub compute_mass: u64,
    pub block_hash: RpcHash,
    pub block_time: u64,
}

impl Serializer for RpcTransactionVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(u64, &self.compute_mass, writer)?;
        store!(RpcHash, &self.block_hash, writer)?;
        store!(u64, &self.block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let hash = load!(RpcHash, reader)?;
        let compute_mass = load!(u64, reader)?;
        let block_hash = load!(RpcHash, reader)?;
        let block_time = load!(u64, reader)?;

        Ok(Self { transaction_id, hash, compute_mass, block_hash, block_time })
    }
}

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptedTransactionIds {
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}
