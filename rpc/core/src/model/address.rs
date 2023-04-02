use crate::{RpcTransactionOutpoint, RpcUtxoEntry};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

pub type RpcAddress = kaspa_addresses::Address;

///
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxosByAddressesEntry {
    pub address: Option<RpcAddress>,
    pub outpoint: RpcTransactionOutpoint,
    pub utxo_entry: RpcUtxoEntry,
}

///
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct RpcBalancesByAddressesEntry {
    pub address: RpcAddress,

    /// Balance of `address` if available
    pub balance: Option<u64>,
}
