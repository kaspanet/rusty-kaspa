use crate::{prelude::RpcHash, RpcBlueWorkType};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockHeader {
    pub version: u32, // TODO: change to u16
    pub parents: Vec<RpcBlockLevelParents>,
    pub hash_merkle_root: RpcHash,
    pub accepted_id_merkle_root: RpcHash,
    pub utxo_commitment: RpcHash,
    pub timestamp: i64, // TODO: change to u64
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: RpcBlueWorkType,
    pub pruning_point: RpcHash,
    pub blue_score: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockLevelParents {
    pub parent_hashes: Vec<RpcHash>,
}
