use crate::{RpcTransactionOutpoint, RpcUtxoEntry};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

pub type RpcAddress = kaspa_addresses::Address;

/// Represents a UTXO entry of an address returned by the `GetUtxosByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxosByAddressesEntry {
    pub address: Option<RpcAddress>,
    pub outpoint: RpcTransactionOutpoint,
    pub utxo_entry: RpcUtxoEntry,
}

impl Serializer for RpcUtxosByAddressesEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?; // version
        store!(Option<RpcAddress>, &self.address, writer)?;
        serialize!(RpcTransactionOutpoint, &self.outpoint, writer)?;
        serialize!(RpcUtxoEntry, &self.utxo_entry, writer)
    }
}

impl Deserializer for RpcUtxosByAddressesEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version: u8 = load!(u8, reader)?;
        let address = load!(Option<RpcAddress>, reader)?;
        let outpoint = deserialize!(RpcTransactionOutpoint, reader)?;
        let utxo_entry = deserialize!(RpcUtxoEntry, reader)?;
        Ok(Self { address, outpoint, utxo_entry })
    }
}

/// Represents a balance of an address returned by the `GetBalancesByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBalancesByAddressesEntry {
    pub address: RpcAddress,

    /// Balance of `address` if available
    pub balance: Option<u64>,
}

impl Serializer for RpcBalancesByAddressesEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?; // version
        store!(RpcAddress, &self.address, writer)?;
        store!(Option<u64>, &self.balance, writer)
    }
}

impl Deserializer for RpcBalancesByAddressesEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version: u8 = load!(u8, reader)?;
        let address = load!(RpcAddress, reader)?;
        let balance = load!(Option<u64>, reader)?;
        Ok(Self { address, balance })
    }
}
