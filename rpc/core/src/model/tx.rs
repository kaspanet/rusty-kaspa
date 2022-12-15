use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use consensus_core::tx::{TransactionId, TransactionOutpoint};
use serde::{Deserialize, Serialize};

use crate::prelude::{RpcHash, RpcHexData, RpcScriptClass, RpcSubnetworkId};

/// Represents the ID of a Kaspa transaction
pub type RpcTransactionId = TransactionId;

pub type RpcScriptVec = RpcHexData;

/// Represents a Kaspad ScriptPublicKey
///
/// This should be an alias of [`consensus_core::tx::ScriptPublicKey`] but
/// is not because its script field of type [`consensus_core::tx::ScriptVec`]
/// is a `smallvec::SmallVec` which does not implement the borsh traits.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcScriptPublicKey {
    pub version: u16,
    pub script_public_key: RpcHexData,
}

/// Holds details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
///
/// This should be an alias of [`consensus_core::tx::UtxoEntry`] but is not
///  because of the indirectuse of a `smallvec::SmallVec` by `script_public_key`.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntry {
    pub amount: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

/// Represents a Kaspa transaction outpoint
pub type RpcTransactionOutpoint = TransactionOutpoint;

/// Represents a Kaspa transaction input
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: RpcTransactionOutpoint,
    pub signature_script: RpcHexData,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

/// Represent Kaspa transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub value: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub verbose_data: Option<RpcTransactionOutputVerboseData>,
}

/// Represent Kaspa transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: RpcScriptClass,

    // TODO: change the type of this field for a better binary representation
    pub script_public_key_address: String,
}

/// Represents a Kaspa transaction
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: u16,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: RpcSubnetworkId,
    pub gas: u64,
    pub payload: RpcHexData,
    pub verbose_data: Option<RpcTransactionVerboseData>,
}

/// Represent Kaspa transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    pub mass: u64,
    pub block_hash: RpcHash,
    pub block_time: u64,
}
