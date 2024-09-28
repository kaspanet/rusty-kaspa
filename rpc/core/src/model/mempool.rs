use super::RpcAddress;
use super::RpcTransaction;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Serializer for RpcMempoolEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u64, &self.fee, writer)?;
        serialize!(RpcTransaction, &self.transaction, writer)?;
        store!(bool, &self.is_orphan, writer)
    }
}

impl Deserializer for RpcMempoolEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let fee = load!(u64, reader)?;
        let transaction = deserialize!(RpcTransaction, reader)?;
        let is_orphan = load!(bool, reader)?;
        Ok(Self { fee, transaction, is_orphan })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Serializer for RpcMempoolEntryByAddress {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(RpcAddress, &self.address, writer)?;
        serialize!(Vec<RpcMempoolEntry>, &self.sending, writer)?;
        serialize!(Vec<RpcMempoolEntry>, &self.receiving, writer)
    }
}

impl Deserializer for RpcMempoolEntryByAddress {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let address = load!(RpcAddress, reader)?;
        let sending = deserialize!(Vec<RpcMempoolEntry>, reader)?;
        let receiving = deserialize!(Vec<RpcMempoolEntry>, reader)?;
        Ok(Self { address, sending, receiving })
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(typescript_custom_section)]
        const TS_MEMPOOL_ENTRY: &'static str = r#"
            /**
             * Mempool entry.
             * 
             * @category Node RPC
             */
            export interface IMempoolEntry {
                fee : bigint;
                transaction : ITransaction;
                isOrphan : boolean;
            }
        "#;
    }
}
