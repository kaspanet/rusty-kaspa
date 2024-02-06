use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{RpcBlock, RpcHash, RpcTransaction};

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfirmedData {
    pub confirmations: u64,
    pub blue_score: u64,
    pub daa_score: u64,
    pub timestamp: u64,
    pub chain_block_hash: RpcHash,
    pub chain_block: Option<RpcBlock>,
    pub merge_set_block_acceptance_data: Vec<RpcMergeSetBlockAcceptanceData>,
}

impl RpcConfirmedData {
    pub fn new(
        confirmations: u64,
        blue_score: u64,
        daa_score: u64,
        timestamp: u64,
        chain_block_hash: RpcHash,
        chain_block: Option<RpcBlock>,
        merge_set_block_acceptance_data: Vec<RpcMergeSetBlockAcceptanceData>,
    ) -> Self {
        Self { confirmations, blue_score, timestamp, daa_score, chain_block_hash, chain_block, merge_set_block_acceptance_data }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcMergeSetBlockAcceptanceData {
    pub block_hash: Option<RpcHash>,
    pub block: Option<RpcBlock>,
    pub accepted_transaction_ids: Vec<RpcHash>,
    pub accepted_transactions: Vec<RpcTransaction>,
}

impl RpcMergeSetBlockAcceptanceData {
    pub fn new(
        block_hash: Option<RpcHash>,
        block: Option<RpcBlock>,
        accepted_transaction_ids: Vec<RpcHash>,
        accepted_transactions: Vec<RpcTransaction>,
    ) -> Self {
        Self { block_hash, block, accepted_transaction_ids, accepted_transactions }
    }
}
