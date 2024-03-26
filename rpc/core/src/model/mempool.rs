use super::RpcAddress;
use super::RpcTransaction;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RpcMempoolEntry {
    pub fee: u64,
    pub transaction: RpcTransaction,
    pub is_orphan: bool,
}

impl RpcMempoolEntry {
    pub fn new(fee: u64, transaction: RpcTransaction, is_orphan: bool) -> Self {
        Self { fee, transaction, is_orphan }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RpcMempoolEntryByAddress {
    pub address: RpcAddress,
    pub sending: Vec<RpcMempoolEntry>,
    pub receiving: Vec<RpcMempoolEntry>,
}

impl RpcMempoolEntryByAddress {
    pub fn new(address: RpcAddress, sending: Vec<RpcMempoolEntry>, receiving: Vec<RpcMempoolEntry>) -> Self {
        Self { address, sending, receiving }
    }
}
