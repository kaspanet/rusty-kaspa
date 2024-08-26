use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcHash};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcBlock, protowire::RpcBlock, {
    Self {
        header: Some(protowire::RpcBlockHeader::from(&item.header)),
        transactions: item.transactions.iter().map(protowire::RpcTransaction::from).collect(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcRawBlock, protowire::RpcBlock, {
    Self {
        header: Some(protowire::RpcBlockHeader::from(&item.header)),
        transactions: item.transactions.iter().map(protowire::RpcTransaction::from).collect(),
        verbose_data: None,
    }
});

from!(item: &kaspa_rpc_core::RpcBlockVerboseData, protowire::RpcBlockVerboseData, {
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
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBlock, kaspa_rpc_core::RpcBlock, {
    Self {
        header: item
            .header
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?
            .try_into()?,
        transactions: item.transactions.iter().map(kaspa_rpc_core::RpcTransaction::try_from).collect::<Result<Vec<_>, _>>()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcBlockVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcBlock, kaspa_rpc_core::RpcRawBlock, {
    Self {
        header: item
            .header
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?
            .try_into()?,
        transactions: item.transactions.iter().map(kaspa_rpc_core::RpcTransaction::try_from).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::RpcBlockVerboseData, kaspa_rpc_core::RpcBlockVerboseData, {
    Self {
        hash: RpcHash::from_str(&item.hash)?,
        difficulty: item.difficulty,
        selected_parent_hash: RpcHash::from_str(&item.selected_parent_hash)?,
        transaction_ids: item
            .transaction_ids
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        is_header_only: item.is_header_only,
        blue_score: item.blue_score,
        children_hashes: item
            .children_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        merge_set_blues_hashes: item
            .merge_set_blues_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        merge_set_reds_hashes: item
            .merge_set_reds_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        is_chain_block: item.is_chain_block,
    }
});
