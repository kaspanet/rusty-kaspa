use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockCount {
    pub header_count: u64,
    pub block_count: u64,
}

impl BlockCount {
    pub fn new(block_count: u64, header_count: u64) -> Self {
        Self { block_count, header_count }
    }
}
