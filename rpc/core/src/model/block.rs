use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::prelude::{RpcHash, RpcHeader, RpcTransaction};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlock {
    pub header: RpcHeader,
    pub transactions: Vec<RpcTransaction>,
    pub verbose_data: Option<RpcBlockVerboseData>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockVerboseData {
    pub hash: RpcHash,
    pub difficulty: f64,
    pub selected_parent_hash: RpcHash,
    pub transaction_ids: Vec<RpcHash>,
    pub is_header_only: bool,
    pub blue_score: u64,
    pub children_hashes: Vec<RpcHash>,
    pub merge_set_blues_hashes: Vec<RpcHash>,
    pub merge_set_reds_hashes: Vec<RpcHash>,
    pub is_chain_block: bool,
}
