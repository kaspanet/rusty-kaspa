use std::sync::Arc;

use crate::{GetBlockTemplateResponse, RpcBlock, RpcError, RpcResult, RpcTransaction};
use consensus_core::block::{Block, BlockTemplate, MutableBlock};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: (*item.header).clone(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.clone(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            verbose_data: None,
        }
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
            header: Arc::new(item.header.clone()),
            transactions: Arc::new(
                item.transactions
                    .iter()
                    .map(consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<consensus_core::tx::Transaction>>>()?,
            ),
        })
    }
}
