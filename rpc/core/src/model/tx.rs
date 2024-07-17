use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptVec, TransactionId, TransactionIndexType, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_index_core::models::txindex::AcceptanceDataIndexType;
use serde::{Deserialize, Serialize};

use crate::prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId};

/// Represents the ID of a Kaspa transaction
pub type RpcTransactionId = TransactionId;

pub type RpcScriptVec = ScriptVec;
pub type RpcScriptPublicKey = ScriptPublicKey;
pub type RpcUtxoEntry = UtxoEntry;

/// Represents a Kaspa transaction outpoint
pub type RpcTransactionOutpoint = TransactionOutpoint;

/// Represents a Kaspa transaction input
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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
            previous_outpoint: input.previous_outpoint,
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

/// Represent Kaspa transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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

    pub fn populate_verbose_data(&mut self, verbose_data: RpcTransactionOutputVerboseData) {
        self.verbose_data = Some(verbose_data);
    }
}

impl From<TransactionOutput> for RpcTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key, verbose_data: None }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionData {
    pub transaction: Option<RpcTransaction>,
    pub inclusion_data: Option<RpcTransactionInclusionData>,
    pub acceptance_data: Option<RpcTransactionAcceptanceData>,
}
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInclusionData {
    pub block_hash: RpcHash,
    pub transaction_index: TransactionIndexType,
    pub timestamp: u64,
    pub daa_score: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionAcceptanceData {
    pub block_hash: RpcHash,
    pub acceptance_data_index: AcceptanceDataIndexType,
    pub timestamp: u64,
    pub daa_score: u64,  // use this as indication for network time, NOT confirmation counting!
    pub blue_score: u64, // use this for Confirmation counting!
}

/// Represent Kaspa transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: RpcScriptClass,
    pub script_public_key_address: Address,
}

/// Represents a Kaspa transaction
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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

impl RpcTransaction {
    pub fn populate_verbose_data(&mut self, verbose_data: RpcTransactionVerboseData) {
        self.verbose_data = Some(verbose_data);
    }
}

/// Represent Kaspa transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    pub mass: u64,
    // TODO: deprecate this field, use `RpcTransactionData.inclusion_data` instead, for next rpc breaking release
    pub block_hash: RpcHash, // including block hash
    // TODO: deprecate this field, use `RpcTransactionData.inclusion_data` instead, for next rpc breaking release
    pub block_time: u64, // including block header unix ts in milliseconds
}
/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptedTransactionIds {
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}
