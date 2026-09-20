use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::{RpcBlockVerboseData, RpcOptionalHeader, RpcOptionalTransaction};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalBlock {
    pub header: Option<RpcOptionalHeader>,
    pub transactions: Vec<RpcOptionalTransaction>,
    pub verbose_data: Option<RpcBlockVerboseData>,
}

impl Serializer for RpcOptionalBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Option<RpcOptionalHeader>, &self.header, writer)?;
        serialize!(Vec<RpcOptionalTransaction>, &self.transactions, writer)?;
        serialize!(Option<RpcBlockVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcOptionalBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let header = Some(deserialize!(RpcOptionalHeader, reader)?);
        let transactions = deserialize!(Vec<RpcOptionalTransaction>, reader)?;
        let verbose_data = deserialize!(Option<RpcBlockVerboseData>, reader)?;
        Ok(Self { header, transactions, verbose_data })
    }
}
