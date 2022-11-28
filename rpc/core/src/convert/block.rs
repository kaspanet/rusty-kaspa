use std::sync::Arc;

use crate::{GetBlockTemplateResponse, RpcBlock, RpcBlockVerboseData, RpcError, RpcResult};
use consensus_core::{
    block::{Block, MutableBlock},
    stubs::BlockTemplate,
};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self { header: (&*item.header).into(), transactions: vec![], verbose_data: item.into() }
    }
}

impl From<&Block> for RpcBlockVerboseData {
    fn from(item: &Block) -> Self {
        // TODO: Fill all fields with real values.
        // see kaspad\app\rpc\rpccontext\verbosedata.go PopulateBlockWithVerboseData
        Self {
            // TODO: determine if we can safely use item.header.hash instead
            hash: consensus_core::hashing::header::hash(&item.header),
            difficulty: 0.into(),
            selected_parent_hash: 0.into(),
            transaction_ids: vec![],
            is_header_only: true,
            blue_score: 0u64,
            children_hashes: vec![],
            merge_set_blues_hashes: vec![],
            merge_set_reds_hashes: vec![],
            is_chain_block: false,
        }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self { header: (&item.header).into(), transactions: vec![], verbose_data: (&item.clone().to_immutable()).into() }
    }
}

impl From<&BlockTemplate> for GetBlockTemplateResponse {
    fn from(item: &BlockTemplate) -> Self {
        Self {
            block: (&item.block).into(),
            // TODO: either call some Block.is_synced() if/when available or implement
            // a functional equivalent here based on item.selected_parent_timestamp
            is_synced: true,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: &RpcBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new((&item.header).try_into()?),

            // TODO: Implement converters for all tx structs and fill transactions
            // with real values.
            transactions: Arc::new(vec![]), // FIXME
        })
    }
}
