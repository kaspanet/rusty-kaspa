use crate::{RpcAddress, RpcError, RpcTransactionOutpoint};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_index_core::indexed_utxos::UtxoPageCursor;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcGetUtxosByAddressesCursor {
    pub start_address: Option<RpcAddress>,
    pub start_daa_score: u64,
    pub start_outpoint: Option<RpcTransactionOutpoint>,
}

impl RpcGetUtxosByAddressesCursor {
    pub fn new(start_address: Option<RpcAddress>, start_daa_score: u64, start_outpoint: Option<RpcTransactionOutpoint>) -> Self {
        Self { start_address, start_daa_score, start_outpoint }
    }
}

impl Serializer for RpcGetUtxosByAddressesCursor {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcAddress>, &self.start_address, writer)?;
        store!(u64, &self.start_daa_score, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.start_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcGetUtxosByAddressesCursor {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version: u8 = load!(u8, reader)?;
        let start_address: Option<RpcAddress> = load!(Option<RpcAddress>, reader)?;
        let start_daa_score: u64 = load!(u64, reader)?;
        let start_outpoint: Option<RpcTransactionOutpoint> = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        Ok(Self { start_address, start_daa_score, start_outpoint })
    }
}

impl TryFrom<(&UtxoPageCursor, Prefix)> for RpcGetUtxosByAddressesCursor {
    type Error = RpcError;

    fn try_from((cursor, prefix): (&UtxoPageCursor, Prefix)) -> Result<Self, RpcError> {
        Ok(Self {
            start_address: Some(
                extract_script_pub_key_address(&cursor.script_public_key, prefix).map_err(|e| RpcError::General(e.to_string()))?,
            ),
            start_daa_score: cursor.daa_score,
            start_outpoint: Some((cursor.transaction_outpoint).into()),
        })
    }
}

impl TryFrom<RpcGetUtxosByAddressesCursor> for UtxoPageCursor {
    type Error = RpcError;

    fn try_from(cursor: RpcGetUtxosByAddressesCursor) -> Result<Self, RpcError> {
        let start_address = cursor
            .start_address
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcGetUtxosByAddressesCursor".to_string(), "startAddress".to_string()))?;

        Ok(Self {
            script_public_key: pay_to_address_script(&start_address),
            daa_score: cursor.start_daa_score,
            transaction_outpoint: cursor.start_outpoint.unwrap_or(TransactionOutpoint::EMPTY.into()).into(),
        })
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(typescript_custom_section)]
        const TS_RPC_GET_UTXOS_BY_ADDRESSES_CURSOR: &'static str = r#"
            /**
             * Cursor for paginating through UTXOs by address.
             *
             * @category Node RPC
             */
            export interface IRpcGetUtxosByAddressesCursor {
                startAddress? : Address | string;
                startDaaScore : bigint;
                startOutpoint? : ITransactionOutpoint;
            }
        "#;
    }
}
