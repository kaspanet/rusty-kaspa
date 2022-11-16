use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use consensus_core::tx::TransactionId;
use serde::{Deserialize, Serialize};

use crate::prelude::{RpcHash, RpcHexData, RpcScriptClass, RpcSubnetworkId};

pub type RpcTransactionId = TransactionId;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: u32,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: RpcSubnetworkId,
    pub gas: u64,
    pub payload: RpcHexData,
    pub verbose_data: RpcTransactionVerboseData,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: RpcOutpoint,
    pub signature_script: RpcHexData,
    pub sequence: u64,
    pub sig_op_count: u32,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub amount: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub verbose_data: RpcTransactionOutputVerboseData,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcOutpoint {
    pub transaction_id: RpcTransactionId,
    pub index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntry {
    pub amount: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcScriptPublicKey {
    pub script_public_key: RpcHexData,
    pub version: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    pub mass: u64,
    pub block_hash: RpcHash,
    pub block_time: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: RpcScriptClass,
    pub script_public_key_address: String, // FIXME ? (investigate /crpyto/addresses/src/lib.rs)
}

// #[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
// #[serde(rename_all = "camelCase")]
// pub struct RpcAddress {
//     pub prefix: u32,
//     pub public_key: ScriptKey,
// }
