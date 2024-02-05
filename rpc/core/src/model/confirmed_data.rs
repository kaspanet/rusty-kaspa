use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{RpcBlock, RpcHash, RpcTransaction};

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfirmedData {
    pub accepting_blue_score: u64,
    pub confirmations: u64,
    pub chain_block_hash: Option<RpcHash>,
    pub chain_block_full: Option<RpcBlock>,
    pub merged_block_hashes: Option<Vec<RpcHash>>,
    pub merged_blocks_full: Option<Vec<RpcBlock>>,
    pub accepted_transactions: Option<Vec<RpcHash>>,
    pub accepted_transactions_full: Option<Vec<RpcTransaction>>,
}

impl RpcConfirmedData {
    pub fn new(
        accepting_blue_score: u64,
        confirmations: u64,
        chain_block_hash: Option<RpcHash>,
        chain_block_full: Option<RpcBlock>,
        merged_block_hashes: Option<Vec<RpcHash>>,
        merged_blocks_full: Option<Vec<RpcBlock>>,
        accepted_transactions: Option<Vec<RpcHash>>,
        accepted_transactions_full: Option<Vec<RpcTransaction>>,
    ) -> Self {
        Self {
            accepting_blue_score,
            confirmations,
            chain_block_hash,
            chain_block_full,
            merged_block_hashes,
            merged_blocks_full,
            accepted_transactions,
            accepted_transactions_full,
        }
    }
}
