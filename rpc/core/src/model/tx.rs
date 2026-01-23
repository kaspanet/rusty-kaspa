use crate::{
    RpcOptionalHeader, RpcOptionalTransaction,
    prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId},
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, ScriptVec, TransactionId, TransactionIndexType, TransactionInput, TransactionOutpoint,
    TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_utils::{hex::ToHex, serde_bytes_fixed_ref};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use workflow_serializer::prelude::*;

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
    pub covenant_id: Option<RpcHash>,
}

impl RpcUtxoEntry {
    pub fn new(
        amount: u64,
        script_public_key: ScriptPublicKey,
        block_daa_score: u64,
        is_coinbase: bool,
        covenant_id: Option<RpcHash>,
    ) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase, covenant_id }
    }
}

impl From<UtxoEntry> for RpcUtxoEntry {
    fn from(entry: UtxoEntry) -> Self {
        Self {
            amount: entry.amount,
            script_public_key: entry.script_public_key,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
            covenant_id: entry.covenant_id,
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
            covenant_id: entry.covenant_id,
        }
    }
}

impl Serializer for RpcUtxoEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(ScriptPublicKey, &self.script_public_key, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(bool, &self.is_coinbase, writer)?;
        store!(Option<RpcHash>, &self.covenant_id, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let amount = load!(u64, reader)?;
        let script_public_key = load!(ScriptPublicKey, reader)?;
        let block_daa_score = load!(u64, reader)?;
        let is_coinbase = load!(bool, reader)?;
        let covenant_id = if version > 1 { load!(Option<RpcHash>, reader)? } else { None };

        Ok(Self { amount, script_public_key, block_daa_score, is_coinbase, covenant_id })
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: RpcTransactionOutpoint,
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

impl std::fmt::Debug for RpcTransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &self.signature_script.to_hex())
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
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
    pub covenant: Option<RpcCovenantBinding>,
}

impl RpcTransactionOutput {
    pub fn from_transaction_outputs(other: Vec<TransactionOutput>) -> Vec<Self> {
        other.into_iter().map(Self::from).collect()
    }
}

impl From<TransactionOutput> for RpcTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self {
            value: output.value,
            script_public_key: output.script_public_key,
            verbose_data: None,
            covenant: output.covenant.map(Into::into),
        }
    }
}

impl Serializer for RpcTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(u64, &self.value, writer)?;
        store!(RpcScriptPublicKey, &self.script_public_key, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseData>, &self.verbose_data, writer)?;
        serialize!(Option<RpcCovenantBinding>, &self.covenant, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcTransactionOutput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let value = load!(u64, reader)?;
        let script_public_key = load!(RpcScriptPublicKey, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;
        let covenant = if version > 1 { deserialize!(Option<RpcCovenantBinding>, reader)? } else { None };

        Ok(Self { value, script_public_key, verbose_data, covenant })
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
pub struct RpcCovenantBinding(pub CovenantBinding);

impl RpcCovenantBinding {
    pub fn new(authorizing_input: u16, covenant_id: Hash) -> Self {
        Self(CovenantBinding { authorizing_input, covenant_id })
    }
}

impl From<CovenantBinding> for RpcCovenantBinding {
    fn from(value: CovenantBinding) -> Self {
        Self(value)
    }
}

impl From<RpcCovenantBinding> for CovenantBinding {
    fn from(value: RpcCovenantBinding) -> Self {
        value.0
    }
}

impl Serializer for RpcCovenantBinding {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(u16, &self.0.authorizing_input, writer)?;
        store!(Hash, &self.0.covenant_id, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcCovenantBinding {
    fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let authorizing_input = load!(u16, reader)?;
        let covenant_id = load!(Hash, reader)?;
        Ok(Self(CovenantBinding { authorizing_input, covenant_id }))
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
#[derive(Clone, Serialize, Deserialize)]
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

impl std::fmt::Debug for RpcTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransaction")
            .field("version", &self.version)
            .field("lock_time", &self.lock_time)
            .field("subnetwork_id", &self.subnetwork_id)
            .field("gas", &self.gas)
            .field("payload", &self.payload.to_hex())
            .field("mass", &self.mass)
            .field("inputs", &self.inputs) // Inputs and outputs are placed purposely at the end for better debug visibility
            .field("outputs", &self.outputs)
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
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

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcChainBlockAcceptedTransactions {
    pub chain_block_header: RpcOptionalHeader,
    pub accepted_transactions: Vec<RpcOptionalTransaction>,
}

impl Serializer for RpcChainBlockAcceptedTransactions {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcOptionalHeader, &self.chain_block_header, writer)?;
        serialize!(Vec<RpcOptionalTransaction>, &self.accepted_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcChainBlockAcceptedTransactions {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _struct_version = load!(u16, reader)?;
        let chain_block_header = deserialize!(RpcOptionalHeader, reader)?;
        let accepted_transactions = deserialize!(Vec<RpcOptionalTransaction>, reader)?;

        Ok(Self { chain_block_header, accepted_transactions })
    }
}
