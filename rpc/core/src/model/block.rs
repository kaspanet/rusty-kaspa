use super::RpcRawHeader;
use crate::prelude::{RpcHash, RpcHeader, RpcTransaction};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

/// Raw Rpc block type - without a cached header hash and without verbose data.
/// Used for mining APIs (get_block_template & submit_block)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRawBlock {
    pub header: RpcRawHeader,
    pub transactions: Vec<RpcTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlock {
    pub header: RpcHeader,
    pub transactions: Vec<RpcTransaction>,
    pub verbose_data: Option<RpcBlockVerboseData>,
}

impl Serializer for RpcBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcHeader, &self.header, writer)?;
        serialize!(Vec<RpcTransaction>, &self.transactions, writer)?;
        serialize!(Option<RpcBlockVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let header = deserialize!(RpcHeader, reader)?;
        let transactions = deserialize!(Vec<RpcTransaction>, reader)?;
        let verbose_data = deserialize!(Option<RpcBlockVerboseData>, reader)?;

        Ok(Self { header, transactions, verbose_data })
    }
}

impl Serializer for RpcRawBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcRawHeader, &self.header, writer)?;
        serialize!(Vec<RpcTransaction>, &self.transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcRawBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let header = deserialize!(RpcRawHeader, reader)?;
        let transactions = deserialize!(Vec<RpcTransaction>, reader)?;

        Ok(Self { header, transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockVerboseData {
    pub hash: RpcHash,
    pub difficulty: f64,
    pub selected_parent_hash: RpcHash,
    pub transaction_ids: Vec<RpcHash>,
    pub is_header_only: bool,
    pub blue_score: u64,
    pub children_hashes: Vec<RpcHash>,
    pub merge_set_blues_hashes: Vec<RpcHash>,
    pub merge_set_reds_hashes: Vec<RpcHash>,
    pub is_chain_block: bool,
}

impl Serializer for RpcBlockVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(f64, &self.difficulty, writer)?;
        store!(RpcHash, &self.selected_parent_hash, writer)?;
        store!(Vec<RpcHash>, &self.transaction_ids, writer)?;
        store!(bool, &self.is_header_only, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(Vec<RpcHash>, &self.children_hashes, writer)?;
        store!(Vec<RpcHash>, &self.merge_set_blues_hashes, writer)?;
        store!(Vec<RpcHash>, &self.merge_set_reds_hashes, writer)?;
        store!(bool, &self.is_chain_block, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcBlockVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let hash = load!(RpcHash, reader)?;
        let difficulty = load!(f64, reader)?;
        let selected_parent_hash = load!(RpcHash, reader)?;
        let transaction_ids = load!(Vec<RpcHash>, reader)?;
        let is_header_only = load!(bool, reader)?;
        let blue_score = load!(u64, reader)?;
        let children_hashes = load!(Vec<RpcHash>, reader)?;
        let merge_set_blues_hashes = load!(Vec<RpcHash>, reader)?;
        let merge_set_reds_hashes = load!(Vec<RpcHash>, reader)?;
        let is_chain_block = load!(bool, reader)?;

        Ok(Self {
            hash,
            difficulty,
            selected_parent_hash,
            transaction_ids,
            is_header_only,
            blue_score,
            children_hashes,
            merge_set_blues_hashes,
            merge_set_reds_hashes,
            is_chain_block,
        })
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(typescript_custom_section)]
        const TS_BLOCK: &'static str = r#"
        /**
         * Interface defining the structure of a block.
         * 
         * @category Consensus
         */
        export interface IBlock {
            header: IHeader;
            transactions: ITransaction[];
            verboseData?: IBlockVerboseData;
        }

        /**
         * Interface defining the structure of a block verbose data.
         * 
         * @category Node RPC
         */
        export interface IBlockVerboseData {
            hash: HexString;
            difficulty: number;
            selectedParentHash: HexString;
            transactionIds: HexString[];
            isHeaderOnly: boolean;
            blueScore: number;
            childrenHashes: HexString[];
            mergeSetBluesHashes: HexString[];
            mergeSetRedsHashes: HexString[];
            isChainBlock: boolean;
        }

        /**
         * Interface defining the structure of a raw block.
         * 
         * Raw block is a structure used by GetBlockTemplate and SubmitBlock RPCs
         * and differs from `IBlock` in that it does not include verbose data and carries
         * `IRawHeader` that does not include a cached block hash.
         * 
         * @category Consensus
         */
        export interface IRawBlock {
            header: IRawHeader;
            transactions: ITransaction[];
        }

        "#;
    }
}
