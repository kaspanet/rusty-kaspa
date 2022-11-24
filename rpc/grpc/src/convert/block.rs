use crate::protowire;
use rpc_core::{RpcError, RpcHash, RpcResult};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::RpcBlock> for protowire::RpcBlock {
    fn from(item: &rpc_core::RpcBlock) -> Self {
        Self {
            header: Some(protowire::RpcBlockHeader::from(&item.header)),
            transactions: item.transactions.iter().map(protowire::RpcTransaction::from).collect(),
            verbose_data: Some(protowire::RpcBlockVerboseData::from(&item.verbose_data)),
        }
    }
}

impl From<&rpc_core::RpcBlockVerboseData> for protowire::RpcBlockVerboseData {
    fn from(item: &rpc_core::RpcBlockVerboseData) -> Self {
        Self {
            hash: item.hash.to_string(),
            difficulty: item.difficulty,
            selected_parent_hash: item.selected_parent_hash.to_string(),
            transaction_ids: item.transaction_ids.iter().map(|x| x.to_string()).collect(),
            is_header_only: item.is_header_only,
            blue_score: item.blue_score,
            children_hashes: item.children_hashes.iter().map(|x| x.to_string()).collect(),
            merge_set_blues_hashes: item.merge_set_blues_hashes.iter().map(|x| x.to_string()).collect(),
            merge_set_reds_hashes: item.merge_set_reds_hashes.iter().map(|x| x.to_string()).collect(),
            is_chain_block: item.is_chain_block,
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::RpcBlock> for rpc_core::RpcBlock {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlock) -> RpcResult<Self> {
        Ok(Self {
            header: item
                .header
                .as_ref()
                .ok_or_else(|| RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?
                .try_into()?,
            transactions: item
                .transactions
                .iter()
                .map(rpc_core::RpcTransaction::try_from)
                .collect::<RpcResult<Vec<rpc_core::RpcTransaction>>>()?,
            verbose_data: item
                .verbose_data
                .as_ref()
                .ok_or_else(|| RpcError::MissingRpcFieldError("RpcBlock".to_string(), "verbose_data".to_string()))?
                .try_into()?,
        })
    }
}

impl TryFrom<&protowire::RpcBlockVerboseData> for rpc_core::RpcBlockVerboseData {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlockVerboseData) -> RpcResult<Self> {
        Ok(Self {
            hash: RpcHash::from_str(&item.hash)?,
            difficulty: item.difficulty,
            selected_parent_hash: RpcHash::from_str(&item.selected_parent_hash)?,
            transaction_ids: item
                .transaction_ids
                .iter()
                .map(|x| RpcHash::from_str(x))
                .collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?,
            is_header_only: item.is_header_only,
            blue_score: item.blue_score,
            children_hashes: item
                .children_hashes
                .iter()
                .map(|x| RpcHash::from_str(x))
                .collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?,
            merge_set_blues_hashes: item
                .merge_set_blues_hashes
                .iter()
                .map(|x| RpcHash::from_str(x))
                .collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?,
            merge_set_reds_hashes: item
                .merge_set_reds_hashes
                .iter()
                .map(|x| RpcHash::from_str(x))
                .collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?,
            is_chain_block: item.is_chain_block,
        })
    }
}
