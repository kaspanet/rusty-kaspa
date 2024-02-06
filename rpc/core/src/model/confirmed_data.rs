use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{RpcBlock, RpcHash, RpcTransaction};

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfirmedData {
    pub confirmations: u64,
    pub blue_score: u64,
    pub chain_block_hash: RpcHash,
    pub chain_block: Option<RpcBlock>,
    pub merge_set_block_acceptance_data: Vec<RpcMergeSetBlockAcceptanceData>,
    pub verbose_data: Option<RpcConfirmedDataVerboseData>,
}

impl RpcConfirmedData {
    pub fn new(
        confirmations: u64,
        blue_score: u64,
        chain_block_hash: RpcHash,
        chain_block: Option<RpcBlock>,
        merge_set_block_acceptance_data: Vec<RpcMergeSetBlockAcceptanceData>,
        verbose_data: Option<RpcConfirmedDataVerboseData>,
    ) -> Self {
        Self { confirmations, blue_score, chain_block_hash, chain_block, merge_set_block_acceptance_data, verbose_data }
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfirmedDataVerboseData {
    pub daa_score: u64,
    pub timestamp: u64,
    pub pruning_point: RpcHash,
    pub utxo_commitment: RpcHash,
    pub accepted_id_merkle_root: RpcHash,
}

impl RpcConfirmedDataVerboseData {
    pub fn new(
        daa_score: u64,
        timestamp: u64,
        pruning_point: RpcHash,
        utxo_commitment: RpcHash,
        accepted_id_merkle_root: RpcHash,
    ) -> Self {
        Self { daa_score, timestamp, pruning_point, utxo_commitment, accepted_id_merkle_root }
    }
}
