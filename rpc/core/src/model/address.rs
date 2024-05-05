use crate::{RpcTransactionOutpoint, RpcUtxoEntry};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub type RpcAddress = kaspa_addresses::Address;

/// Represents a UTXO entry of an address returned by the `GetUtxosByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxosByAddressesEntry {
    pub address: Option<RpcAddress>,
    pub outpoint: RpcTransactionOutpoint,
    pub utxo_entry: RpcUtxoEntry,
}

/// Represents a balance of an address returned by the `GetBalancesByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBalancesByAddressesEntry {
    pub address: RpcAddress,

    /// Balance of `address` if available
    pub balance: Option<u64>,
}
